package pervius.asm;

import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.InvokeDynamicInsnNode;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.analysis.Analyzer;
import org.objectweb.asm.tree.analysis.BasicVerifier;
import pervius.compile.SourceCompiler;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * 字节码锻造工具
 *
 * 两种模式：
 *   默认模式: stdin 读取 class 字节 → 重算 StackMapTable / max_stack / max_locals → stdout
 *   --patch:  stdin 读取 class 字节 + 方法编辑列表 → 替换指定方法字节码 → stdout
 *
 * 用法:
 *   java -jar classforge.jar [--classpath jar_path]
 *   java -jar classforge.jar --patch [--classpath jar_path]
 *
 * @author sky
 */
public class ClassForge {

    public static void main(String[] args) throws Exception {
        for (String arg : args) {
            if ("--compile-kt".equals(arg)) {
                try {
                    byte[] output = pervius.compile.KotlincCompiler.run(args);
                    System.out.write(output);
                    System.out.flush();
                } catch (Throwable e) {
                    System.err.println("ClassForge error: " + e.getMessage());
                    e.printStackTrace(System.err);
                    System.exit(2);
                }
                return;
            }
            if ("--compile".equals(arg)) {
                SourceCompiler.run(args);
                return;
            }
        }
        String classpathJar = null;
        boolean patchMode = false;
        for (int i = 0; i < args.length; i++) {
            if ("--classpath".equals(args[i]) && i + 1 < args.length) {
                classpathJar = args[++i];
            } else if ("--patch".equals(args[i])) {
                patchMode = true;
            }
        }
        try {
            byte[] output;
            if (patchMode) {
                output = runPatch(classpathJar);
            } else {
                output = runReframe(classpathJar);
            }
            System.out.write(output);
            System.out.flush();
        } catch (Exception e) {
            System.err.println("ASM error: " + e.getMessage());
            e.printStackTrace(System.err);
            System.exit(2);
        }
    }

    /**
     * 默认模式：stdin 原始 class 字节 → 重算帧 → stdout
     */
    private static byte[] runReframe(String classpathJar) throws Exception {
        byte[] input = readAll(System.in);
        if (input.length == 0) {
            System.err.println("No input");
            System.exit(1);
        }
        ClassReader cr = new ClassReader(input);
        try (JarClassWriter cw = new JarClassWriter(cr, ClassWriter.COMPUTE_FRAMES, classpathJar)) {
            cr.accept(cw, ClassReader.SKIP_FRAMES);
            return cw.toByteArray();
        }
    }

    /**
     * --patch 模式：读取 class 字节 + 方法编辑 → 替换字节码 → stdout
     *
     * 流式复制：未修改方法直接字节拷贝（保留原始帧），
     * 仅对修改的方法触发 COMPUTE_FRAMES 重算帧。
     *
     * 协议（stdin, big-endian）：
     *   [4B class 长度][class 字节]
     *   [4B 编辑数]
     *   每条编辑：[2B name 长度][name][2B desc 长度][desc][4B code 长度][code 文本]
     */
    private static byte[] runPatch(String classpathJar) throws Exception {
        DataInputStream dis = new DataInputStream(System.in);
        int classLen = dis.readInt();
        byte[] classData = new byte[classLen];
        dis.readFully(classData);
        int editCount = dis.readInt();
        Map<String, String> edits = new LinkedHashMap<>();
        for (int i = 0; i < editCount; i++) {
            String name = readPrefixedString(dis);
            String desc = readPrefixedString(dis);
            String code = readPrefixedStringU32(dis);
            edits.put(name + desc, code);
        }
        // 第一遍：只读取需要 patch 的方法（跳过无关方法，避免参数注解数量不匹配触发 ASM AIOOBE）
        ClassNode cn = new ClassNode(Opcodes.ASM9) {
            @Override
            public MethodVisitor visitMethod(int access, String name, String descriptor, String signature, String[] exceptions) {
                if (!edits.containsKey(name + descriptor)) {
                    return null;
                }
                return super.visitMethod(access, name, descriptor, signature, exceptions);
            }
        };
        new ClassReader(classData).accept(cn, ClassReader.SKIP_FRAMES);
        Map<String, MethodNode> modified = new LinkedHashMap<>();
        for (MethodNode mn : cn.methods) {
            String key = mn.name + mn.desc;
            String code = edits.get(key);
            if (code == null) continue;
            List<InvokeDynamicInsnNode> originalDynamic = new ArrayList<>();
            for (AbstractInsnNode insn = mn.instructions.getFirst(); insn != null; insn = insn.getNext()) {
                if (insn instanceof InvokeDynamicInsnNode) {
                    originalDynamic.add((InvokeDynamicInsnNode) insn);
                }
            }
            BytecodeAssembler.assemble(mn, code, originalDynamic);
            modified.put(key, mn);
        }
        // 用 COMPUTE_MAXS 预算 maxStack/maxLocals，再交给 Analyzer 校验
        computeMaxs(cn, modified);
        Analyzer<?> analyzer = new Analyzer<>(new BasicVerifier());
        for (MethodNode mn : modified.values()) {
            analyzer.analyze(cn.name, mn);
        }
        // 第二遍：流式复制——未修改方法直接字节拷贝（保留原始帧），修改的方法由 ASM 重算
        ClassReader cr = new ClassReader(classData);
        try {
            return writeClass(cr, modified, ClassWriter.COMPUTE_FRAMES, classpathJar);
        } catch (Exception e) {
            // COMPUTE_FRAMES 失败（类层次无法解析），回退到 COMPUTE_MAXS
            System.err.println("COMPUTE_FRAMES failed, falling back to COMPUTE_MAXS: " + e.getMessage());
            return writeClass(cr, modified, ClassWriter.COMPUTE_MAXS, classpathJar);
        }
    }

    /**
     * 流式写出 class：未修改方法直接字节拷贝，修改的方法由指定 flags 重算
     */
    private static byte[] writeClass(
            ClassReader cr,
            Map<String, MethodNode> modified,
            int writerFlags,
            String classpathJar
    ) throws Exception {
        try (JarClassWriter cw = new JarClassWriter(cr, writerFlags, classpathJar)) {
            cr.accept(new ClassVisitor(Opcodes.ASM9, cw) {
                @Override
                public MethodVisitor visitMethod(int access, String name, String descriptor, String signature, String[] exceptions) {
                    MethodNode mod = modified.get(name + descriptor);
                    if (mod != null) {
                        // 在原始位置写入修改后的方法内容
                        MethodVisitor mv = super.visitMethod(access, name, descriptor, signature, exceptions);
                        mod.accept(mv);
                        return null;
                    }
                    return super.visitMethod(access, name, descriptor, signature, exceptions);
                }
            }, ClassReader.SKIP_FRAMES);
            return cw.toByteArray();
        }
    }

    /**
     * 用临时 ClassWriter(COMPUTE_MAXS) 预算 maxStack/maxLocals，
     * 回填到 MethodNode，供后续 Analyzer 校验使用。
     */
    private static void computeMaxs(ClassNode cn, Map<String, MethodNode> modified) {
        ClassWriter cw = new ClassWriter(ClassWriter.COMPUTE_MAXS);
        String[] ifaces = cn.interfaces != null ? cn.interfaces.toArray(new String[0]) : null;
        cw.visit(cn.version, cn.access, cn.name, cn.signature, cn.superName, ifaces);
        for (MethodNode mn : modified.values()) {
            mn.accept(cw);
        }
        ClassReader cr = new ClassReader(cw.toByteArray());
        ClassNode tmp = new ClassNode();
        cr.accept(tmp, ClassReader.SKIP_FRAMES);
        for (MethodNode tmn : tmp.methods) {
            MethodNode orig = modified.get(tmn.name + tmn.desc);
            if (orig != null) {
                orig.maxStack = tmn.maxStack;
                orig.maxLocals = tmn.maxLocals;
            }
        }
    }

    private static String readPrefixedString(DataInputStream dis) throws IOException {
        int len = dis.readUnsignedShort();
        byte[] buf = new byte[len];
        dis.readFully(buf);
        return new String(buf, StandardCharsets.UTF_8);
    }

    private static String readPrefixedStringU32(DataInputStream dis) throws IOException {
        int len = dis.readInt();
        byte[] buf = new byte[len];
        dis.readFully(buf);
        return new String(buf, StandardCharsets.UTF_8);
    }

    private static byte[] readAll(InputStream in) throws IOException {
        ByteArrayOutputStream buf = new ByteArrayOutputStream(4096);
        byte[] tmp = new byte[4096];
        int n;
        while ((n = in.read(tmp)) != -1) {
            buf.write(tmp, 0, n);
        }
        return buf.toByteArray();
    }
}
