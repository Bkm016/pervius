package pervius.asm;

import org.objectweb.asm.Handle;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.tree.*;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * 将 Pervius 字节码文本解析为 ASM MethodNode 指令
 *
 * 解析 Recaf 风格大写操作码 + 内联 CP 引用格式，
 * 支持 DEFINE 头行、字母标签、LINE 指令、变量名解析。
 * ASM 自动管理常量池，无需手动维护 CP 条目。
 *
 * @author sky
 */
public class BytecodeAssembler {

    /**
     * 解析字节码文本，填充 MethodNode 的指令/异常表/局部变量表
     *
     * @param method          目标 MethodNode（指令和元数据将被替换）
     * @param code            字节码文本
     * @param originalDynamic 原始方法中的 INVOKEDYNAMIC 指令（按出现顺序），用于保留 bootstrap 信息
     * @param originalMethods 原始方法中的普通方法调用指令（按出现顺序），用于保留接口调用常量池类型
     */
    public static void assemble(
            MethodNode method,
            String code,
            List<InvokeDynamicInsnNode> originalDynamic,
            List<MethodInsnNode> originalMethods
    ) {
        // Pass 1: 收集标签定义 + 变量名→槽位映射
        Map<String, LabelNode> labels = collectLabels(code);
        Map<String, Integer> nameToSlot = collectVarNames(code);
        InsnList insns = new InsnList();
        List<TryCatchBlockNode> tryCatch = new ArrayList<>();
        // .var: (slot, name, desc, startLabel, endLabel)
        List<Object[]> vars = new ArrayList<>();
        // .vartype: (slot, name, sig, startLabel, endLabel)
        List<Object[]> varTypes = new ArrayList<>();
        int dynamicIdx = 0;
        int methodInsnIdx = 0;
        LabelNode lastLabel = null;
        String[] lines = code.split("\n");
        int i = 0;
        while (i < lines.length) {
            String t = lines[i].trim();
            i++;
            if (t.isEmpty() || t.startsWith("//")) continue;
            // 跳过 DEFINE 头行
            if (t.startsWith("DEFINE ")) continue;
            // 标签定义
            if (t.endsWith(":") && !t.contains(" ") && t.length() > 1) {
                String name = t.substring(0, t.length() - 1);
                LabelNode label = labels.get(name);
                if (label != null) {
                    insns.add(label);
                    lastLabel = label;
                }
                continue;
            }
            // LINE label num（新格式）
            if (t.startsWith("LINE ")) {
                String[] parts = t.substring(5).trim().split("\\s+");
                LabelNode label = labels.get(parts[0]);
                int lineNum = Integer.parseInt(parts[1]);
                if (label == null) {
                    label = lastLabel;
                }
                if (label == null) {
                    label = new LabelNode();
                    insns.add(label);
                    lastLabel = label;
                }
                insns.add(new LineNumberNode(lineNum, label));
                continue;
            }
            // .line N（旧格式兼容）
            if (t.startsWith(".line ")) {
                int lineNum = Integer.parseInt(t.substring(6).trim());
                if (lastLabel == null) {
                    lastLabel = new LabelNode();
                    insns.add(lastLabel);
                }
                insns.add(new LineNumberNode(lineNum, lastLabel));
                continue;
            }
            // .catch type from to handler
            if (t.startsWith(".catch ")) {
                String[] parts = t.substring(7).trim().split("\\s+");
                String type = "*".equals(parts[0]) ? null : parts[0];
                tryCatch.add(new TryCatchBlockNode(
                        labels.get(parts[1]),
                        labels.get(parts[2]),
                        labels.get(parts[3]),
                        type
                ));
                continue;
            }
            // .var slot name desc start end
            if (t.startsWith(".var ")) {
                String[] parts = t.substring(5).trim().split("\\s+");
                vars.add(new Object[]{
                        Integer.parseInt(parts[0]),
                        parts[1], parts[2],
                        labels.get(parts[3]),
                        labels.get(parts[4])
                });
                continue;
            }
            // .vartype slot name sig start end
            if (t.startsWith(".vartype ")) {
                String[] parts = t.substring(9).trim().split("\\s+");
                varTypes.add(new Object[]{
                        Integer.parseInt(parts[0]),
                        parts[1], parts[2],
                        labels.get(parts[3]),
                        labels.get(parts[4])
                });
                continue;
            }
            // Switch 多行块
            int sp = t.indexOf(' ');
            String opcode = sp == -1 ? t.toUpperCase() : t.substring(0, sp).toUpperCase();
            if ("TABLESWITCH".equals(opcode) || "LOOKUPSWITCH".equals(opcode)) {
                List<String> block = new ArrayList<>();
                block.add(t);
                while (i < lines.length) {
                    String bl = lines[i].trim();
                    i++;
                    block.add(bl);
                    if ("}".equals(bl)) break;
                }
                if ("TABLESWITCH".equals(opcode)) {
                    insns.add(parseTableSwitch(block, labels));
                } else {
                    insns.add(parseLookupSwitch(block, labels));
                }
                lastLabel = null;
                continue;
            }
            // 普通指令
            String operands = sp == -1 ? "" : t.substring(sp + 1).trim();
            AbstractInsnNode insn = parseInstruction(
                    opcode,
                    operands,
                    labels,
                    originalDynamic,
                    dynamicIdx,
                    originalMethods,
                    methodInsnIdx,
                    nameToSlot
            );
            if (insn instanceof InvokeDynamicInsnNode) dynamicIdx++;
            if (insn instanceof MethodInsnNode) methodInsnIdx++;
            insns.add(insn);
            lastLabel = null;
        }
        method.instructions = insns;
        method.tryCatchBlocks = tryCatch;
        method.localVariables = mergeLocalVars(vars, varTypes);
        method.maxStack = 0;
        method.maxLocals = 0;
    }

    /**
     * Pass 1: 扫描标签定义，创建 LabelNode
     */
    private static Map<String, LabelNode> collectLabels(String code) {
        Map<String, LabelNode> labels = new HashMap<>();
        for (String line : code.split("\n")) {
            String t = line.trim();
            if (t.endsWith(":") && !t.contains(" ") && t.length() > 1) {
                labels.put(t.substring(0, t.length() - 1), new LabelNode());
            }
        }
        return labels;
    }

    /**
     * Pass 1: 从 DEFINE 头行和 .var 指令收集变量名→槽位映射
     */
    private static Map<String, Integer> collectVarNames(String code) {
        Map<String, Integer> nameToSlot = new HashMap<>();
        for (String line : code.split("\n")) {
            String t = line.trim();
            if (t.startsWith("DEFINE ")) {
                parseDefineVars(t, nameToSlot);
            } else if (t.startsWith(".var ")) {
                String[] parts = t.substring(5).trim().split("\\s+");
                int slot = Integer.parseInt(parts[0]);
                String name = parts[1];
                nameToSlot.putIfAbsent(name, slot);
            }
        }
        return nameToSlot;
    }

    /**
     * 从 DEFINE 头行提取参数名→槽位映射
     *
     * 格式: DEFINE access name(Type1 param1, Type2 param2)RetType
     */
    private static void parseDefineVars(String line, Map<String, Integer> nameToSlot) {
        int parenOpen = line.indexOf('(');
        int parenClose = line.lastIndexOf(')');
        if (parenOpen < 0 || parenClose < 0 || parenClose <= parenOpen) return;
        // 判断 static
        String beforeParen = line.substring(0, parenOpen).toUpperCase();
        boolean isStatic = beforeParen.contains("STATIC");
        if (!isStatic) {
            nameToSlot.put("this", 0);
        }
        String params = line.substring(parenOpen + 1, parenClose).trim();
        if (params.isEmpty()) return;
        int slot = isStatic ? 0 : 1;
        String[] parts = params.split(",");
        for (String part : parts) {
            part = part.trim();
            int lastSpace = part.lastIndexOf(' ');
            if (lastSpace < 0) continue;
            String type = part.substring(0, lastSpace).trim();
            String name = part.substring(lastSpace + 1).trim();
            nameToSlot.put(name, slot);
            // long/double 占 2 个槽位（数组不算）
            if ("J".equals(type) || "D".equals(type)) {
                slot += 2;
            } else {
                slot += 1;
            }
        }
    }

    /**
     * 解析变量操作数：优先当作数字，回退为变量名查找
     */
    private static int resolveVar(String operand, Map<String, Integer> nameToSlot) {
        String s = operand.trim();
        try {
            return Integer.parseInt(s);
        } catch (NumberFormatException e) {
            Integer slot = nameToSlot.get(s);
            if (slot != null) return slot;
            throw new IllegalArgumentException("Unknown variable: " + s);
        }
    }

    /**
     * 解析单条指令
     */
    private static AbstractInsnNode parseInstruction(
            String opcode,
            String operands,
            Map<String, LabelNode> labels,
            List<InvokeDynamicInsnNode> originalDynamic,
            int dynamicIdx,
            List<MethodInsnNode> originalMethods,
            int methodInsnIdx,
            Map<String, Integer> nameToSlot
    ) {
        // 零操作数指令
        Integer zeroOp = ZERO_OPS.get(opcode);
        if (zeroOp != null) return new InsnNode(zeroOp);
        switch (opcode) {
            // 立即数
            case "BIPUSH": return new IntInsnNode(Opcodes.BIPUSH, Integer.parseInt(operands.trim()));
            case "SIPUSH": return new IntInsnNode(Opcodes.SIPUSH, Integer.parseInt(operands.trim()));
            case "NEWARRAY": return new IntInsnNode(Opcodes.NEWARRAY, parseArrayType(operands.trim()));
            // 局部变量（支持变量名）
            case "ILOAD": return new VarInsnNode(Opcodes.ILOAD, resolveVar(operands, nameToSlot));
            case "LLOAD": return new VarInsnNode(Opcodes.LLOAD, resolveVar(operands, nameToSlot));
            case "FLOAD": return new VarInsnNode(Opcodes.FLOAD, resolveVar(operands, nameToSlot));
            case "DLOAD": return new VarInsnNode(Opcodes.DLOAD, resolveVar(operands, nameToSlot));
            case "ALOAD": return new VarInsnNode(Opcodes.ALOAD, resolveVar(operands, nameToSlot));
            case "ISTORE": return new VarInsnNode(Opcodes.ISTORE, resolveVar(operands, nameToSlot));
            case "LSTORE": return new VarInsnNode(Opcodes.LSTORE, resolveVar(operands, nameToSlot));
            case "FSTORE": return new VarInsnNode(Opcodes.FSTORE, resolveVar(operands, nameToSlot));
            case "DSTORE": return new VarInsnNode(Opcodes.DSTORE, resolveVar(operands, nameToSlot));
            case "ASTORE": return new VarInsnNode(Opcodes.ASTORE, resolveVar(operands, nameToSlot));
            case "RET": return new VarInsnNode(Opcodes.RET, resolveVar(operands, nameToSlot));
            // 分支
            case "IFEQ": return new JumpInsnNode(Opcodes.IFEQ, labels.get(operands.trim()));
            case "IFNE": return new JumpInsnNode(Opcodes.IFNE, labels.get(operands.trim()));
            case "IFLT": return new JumpInsnNode(Opcodes.IFLT, labels.get(operands.trim()));
            case "IFGE": return new JumpInsnNode(Opcodes.IFGE, labels.get(operands.trim()));
            case "IFGT": return new JumpInsnNode(Opcodes.IFGT, labels.get(operands.trim()));
            case "IFLE": return new JumpInsnNode(Opcodes.IFLE, labels.get(operands.trim()));
            case "IF_ICMPEQ": return new JumpInsnNode(Opcodes.IF_ICMPEQ, labels.get(operands.trim()));
            case "IF_ICMPNE": return new JumpInsnNode(Opcodes.IF_ICMPNE, labels.get(operands.trim()));
            case "IF_ICMPLT": return new JumpInsnNode(Opcodes.IF_ICMPLT, labels.get(operands.trim()));
            case "IF_ICMPGE": return new JumpInsnNode(Opcodes.IF_ICMPGE, labels.get(operands.trim()));
            case "IF_ICMPGT": return new JumpInsnNode(Opcodes.IF_ICMPGT, labels.get(operands.trim()));
            case "IF_ICMPLE": return new JumpInsnNode(Opcodes.IF_ICMPLE, labels.get(operands.trim()));
            case "IF_ACMPEQ": return new JumpInsnNode(Opcodes.IF_ACMPEQ, labels.get(operands.trim()));
            case "IF_ACMPNE": return new JumpInsnNode(Opcodes.IF_ACMPNE, labels.get(operands.trim()));
            case "GOTO": return new JumpInsnNode(Opcodes.GOTO, labels.get(operands.trim()));
            case "JSR": return new JumpInsnNode(Opcodes.JSR, labels.get(operands.trim()));
            case "IFNULL": return new JumpInsnNode(Opcodes.IFNULL, labels.get(operands.trim()));
            case "IFNONNULL": return new JumpInsnNode(Opcodes.IFNONNULL, labels.get(operands.trim()));
            // LDC 系列
            case "LDC":
            case "LDC_W":
            case "LDC2_W":
                return new LdcInsnNode(parseLdcValue(operands.trim()));
            // 字段访问: owner.fieldName descriptor
            case "GETSTATIC": return parseFieldInsn(Opcodes.GETSTATIC, operands);
            case "PUTSTATIC": return parseFieldInsn(Opcodes.PUTSTATIC, operands);
            case "GETFIELD": return parseFieldInsn(Opcodes.GETFIELD, operands);
            case "PUTFIELD": return parseFieldInsn(Opcodes.PUTFIELD, operands);
            // 方法调用: owner.methodName(desc)ret
            case "INVOKEVIRTUAL": return parseMethodInsn(Opcodes.INVOKEVIRTUAL, operands, false, originalMethods, methodInsnIdx);
            case "INVOKESPECIAL": return parseMethodInsn(Opcodes.INVOKESPECIAL, operands, false, originalMethods, methodInsnIdx);
            case "INVOKESTATIC": return parseMethodInsn(Opcodes.INVOKESTATIC, operands, false, originalMethods, methodInsnIdx);
            case "INVOKEINTERFACE": return parseMethodInsn(Opcodes.INVOKEINTERFACE, operands, true, originalMethods, methodInsnIdx);
            // INVOKEDYNAMIC: #bsmIdx name(desc)ret → 复用原始 bootstrap 信息
            case "INVOKEDYNAMIC": return parseInvokeDynamic(operands, originalDynamic, dynamicIdx);
            // 类型操作
            case "NEW": return new TypeInsnNode(Opcodes.NEW, operands.trim());
            case "ANEWARRAY": return new TypeInsnNode(Opcodes.ANEWARRAY, operands.trim());
            case "CHECKCAST": return new TypeInsnNode(Opcodes.CHECKCAST, operands.trim());
            case "INSTANCEOF": return new TypeInsnNode(Opcodes.INSTANCEOF, operands.trim());
            // IINC var, increment（支持变量名）
            case "IINC": {
                String[] parts = operands.split(",");
                return new IincInsnNode(
                        resolveVar(parts[0], nameToSlot),
                        Integer.parseInt(parts[1].trim())
                );
            }
            // MULTIANEWARRAY type dims
            case "MULTIANEWARRAY": {
                int lastSp = operands.lastIndexOf(' ');
                return new MultiANewArrayInsnNode(
                        operands.substring(0, lastSp).trim(),
                        Integer.parseInt(operands.substring(lastSp + 1).trim())
                );
            }
            default:
                throw new IllegalArgumentException("Unknown opcode: " + opcode);
        }
    }

    /**
     * 解析 LDC 值：字符串/整数/浮点/类引用
     */
    private static Object parseLdcValue(String s) {
        // 字符串: "..."
        if (s.startsWith("\"") && s.endsWith("\"")) {
            return unescapeString(s.substring(1, s.length() - 1));
        }
        // 后缀也可能出现在内部类名中，数字解析失败时继续按 Class 引用处理。
        // Float: Nf
        if (s.endsWith("f") || s.endsWith("F")) {
            try {
                return Float.parseFloat(s.substring(0, s.length() - 1));
            } catch (NumberFormatException ignored) {}
        }
        // Long: NL
        if (s.endsWith("L") || s.endsWith("l")) {
            try {
                return Long.parseLong(s.substring(0, s.length() - 1));
            } catch (NumberFormatException ignored) {}
        }
        // Integer
        try {
            return Integer.parseInt(s);
        } catch (NumberFormatException ignored) {}
        // Double
        try {
            return Double.parseDouble(s);
        } catch (NumberFormatException ignored) {}
        // Class 引用
        return Type.getObjectType(s);
    }

    /**
     * 解析字段指令: "owner.fieldName descriptor"
     */
    private static FieldInsnNode parseFieldInsn(int opcode, String operands) {
        String s = operands.trim();
        int dot = s.lastIndexOf('.');
        String owner = s.substring(0, dot);
        String rest = s.substring(dot + 1);
        int sp = rest.indexOf(' ');
        String name = rest.substring(0, sp);
        String desc = rest.substring(sp + 1).trim();
        return new FieldInsnNode(opcode, owner, name, desc);
    }

    /**
     * 解析方法调用指令: "owner.methodName(desc)ret"
     */
    private static MethodInsnNode parseMethodInsn(
            int opcode,
            String operands,
            boolean defaultInterface,
            List<MethodInsnNode> originalMethods,
            int methodInsnIdx
    ) {
        String s = operands.trim();
        int paren = s.indexOf('(');
        String beforeParen = s.substring(0, paren);
        String desc = s.substring(paren);
        int dot = beforeParen.lastIndexOf('.');
        String owner = beforeParen.substring(0, dot);
        String name = beforeParen.substring(dot + 1);
        boolean isInterface = resolveOriginalInterfaceFlag(
                opcode,
                owner,
                name,
                desc,
                defaultInterface,
                originalMethods,
                methodInsnIdx
        );
        return new MethodInsnNode(opcode, owner, name, desc, isInterface);
    }

    /**
     * 保留原始方法调用的接口常量池类型
     * INVOKESTATIC 调用接口静态方法时 opcode 仍是 INVOKESTATIC，但常量池必须写成 InterfaceMethodref。
     */
    private static boolean resolveOriginalInterfaceFlag(
            int opcode,
            String owner,
            String name,
            String desc,
            boolean defaultInterface,
            List<MethodInsnNode> originalMethods,
            int methodInsnIdx
    ) {
        if (methodInsnIdx >= originalMethods.size()) {
            return defaultInterface;
        }
        MethodInsnNode original = originalMethods.get(methodInsnIdx);
        if (original.getOpcode() != opcode) {
            return defaultInterface;
        }
        if (!owner.equals(original.owner) || !name.equals(original.name) || !desc.equals(original.desc)) {
            return defaultInterface;
        }
        return original.itf;
    }

    /**
     * 解析 INVOKEDYNAMIC: "#N name(desc)ret"
     * 复用原始方法中第 dynamicIdx 个 INVOKEDYNAMIC 的 bootstrap 信息
     */
    private static AbstractInsnNode parseInvokeDynamic(
            String operands,
            List<InvokeDynamicInsnNode> originalDynamic,
            int dynamicIdx
    ) {
        String s = operands.trim();
        // 跳过 #N 前缀
        int sp = s.indexOf(' ');
        String rest = sp >= 0 ? s.substring(sp + 1).trim() : s;
        int paren = rest.indexOf('(');
        String name = rest.substring(0, paren);
        String desc = rest.substring(paren);
        // 复用原始 bootstrap 信息
        if (dynamicIdx < originalDynamic.size()) {
            InvokeDynamicInsnNode orig = originalDynamic.get(dynamicIdx);
            return new InvokeDynamicInsnNode(name, desc, orig.bsm, orig.bsmArgs);
        }
        // 没有原始信息，创建空 Handle（不应该发生）
        Handle emptyHandle = new Handle(Opcodes.H_INVOKESTATIC, "java/lang/invoke/LambdaMetafactory", "metafactory",
                "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodType;Ljava/lang/invoke/MethodHandle;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/CallSite;",
                false);
        return new InvokeDynamicInsnNode(name, desc, emptyHandle);
    }

    /**
     * TABLESWITCH 多行块解析
     */
    private static TableSwitchInsnNode parseTableSwitch(List<String> block, Map<String, LabelNode> labels) {
        // 第一行: TABLESWITCH { // low to high
        String first = block.get(0);
        int commentStart = first.indexOf("//");
        String comment = first.substring(commentStart + 2).trim();
        String[] range = comment.split("\\s+to\\s+");
        int low = Integer.parseInt(range[0].trim());
        int high = Integer.parseInt(range[1].trim());
        LabelNode dflt = null;
        List<LabelNode> targets = new ArrayList<>();
        for (int i = 1; i < block.size(); i++) {
            String t = block.get(i).trim();
            if ("}".equals(t)) break;
            int colon = t.indexOf(':');
            String key = t.substring(0, colon).trim();
            String label = t.substring(colon + 1).trim();
            if ("default".equals(key)) {
                dflt = labels.get(label);
            } else {
                targets.add(labels.get(label));
            }
        }
        return new TableSwitchInsnNode(low, high, dflt, targets.toArray(new LabelNode[0]));
    }

    /**
     * LOOKUPSWITCH 多行块解析
     */
    private static LookupSwitchInsnNode parseLookupSwitch(List<String> block, Map<String, LabelNode> labels) {
        LabelNode dflt = null;
        List<Integer> keys = new ArrayList<>();
        List<LabelNode> targets = new ArrayList<>();
        for (int i = 1; i < block.size(); i++) {
            String t = block.get(i).trim();
            if ("}".equals(t)) break;
            int colon = t.indexOf(':');
            String key = t.substring(0, colon).trim();
            String label = t.substring(colon + 1).trim();
            if ("default".equals(key)) {
                dflt = labels.get(label);
            } else {
                keys.add(Integer.parseInt(key));
                targets.add(labels.get(label));
            }
        }
        int[] keyArray = new int[keys.size()];
        for (int i = 0; i < keys.size(); i++) keyArray[i] = keys.get(i);
        return new LookupSwitchInsnNode(dflt, keyArray, targets.toArray(new LabelNode[0]));
    }

    /**
     * 合并 .var 和 .vartype 为 LocalVariableNode 列表
     */
    private static List<LocalVariableNode> mergeLocalVars(List<Object[]> vars, List<Object[]> varTypes) {
        List<LocalVariableNode> result = new ArrayList<>();
        // 为每个 .var 创建条目，匹配 .vartype 补充 signature
        for (Object[] v : vars) {
            int slot = (int) v[0];
            String name = (String) v[1];
            String desc = (String) v[2];
            LabelNode start = (LabelNode) v[3];
            LabelNode end = (LabelNode) v[4];
            String signature = null;
            for (Object[] vt : varTypes) {
                if ((int) vt[0] == slot && vt[3] == start && vt[4] == end) {
                    signature = (String) vt[2];
                    break;
                }
            }
            result.add(new LocalVariableNode(name, desc, signature, start, end, slot));
        }
        return result;
    }

    private static int parseArrayType(String s) {
        switch (s) {
            case "boolean": return Opcodes.T_BOOLEAN;
            case "char": return Opcodes.T_CHAR;
            case "float": return Opcodes.T_FLOAT;
            case "double": return Opcodes.T_DOUBLE;
            case "byte": return Opcodes.T_BYTE;
            case "short": return Opcodes.T_SHORT;
            case "int": return Opcodes.T_INT;
            case "long": return Opcodes.T_LONG;
            default: throw new IllegalArgumentException("Unknown array type: " + s);
        }
    }

    private static String unescapeString(String s) {
        StringBuilder sb = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\' && i + 1 < s.length()) {
                char next = s.charAt(++i);
                switch (next) {
                    case 'n': sb.append('\n'); break;
                    case 'r': sb.append('\r'); break;
                    case 't': sb.append('\t'); break;
                    case '\\': sb.append('\\'); break;
                    case '"': sb.append('"'); break;
                    default: sb.append('\\'); sb.append(next); break;
                }
            } else {
                sb.append(c);
            }
        }
        return sb.toString();
    }

    /** 零操作数指令映射 */
    private static final Map<String, Integer> ZERO_OPS = new HashMap<>();
    static {
        ZERO_OPS.put("NOP", Opcodes.NOP);
        ZERO_OPS.put("ACONST_NULL", Opcodes.ACONST_NULL);
        ZERO_OPS.put("ICONST_M1", Opcodes.ICONST_M1);
        ZERO_OPS.put("ICONST_0", Opcodes.ICONST_0);
        ZERO_OPS.put("ICONST_1", Opcodes.ICONST_1);
        ZERO_OPS.put("ICONST_2", Opcodes.ICONST_2);
        ZERO_OPS.put("ICONST_3", Opcodes.ICONST_3);
        ZERO_OPS.put("ICONST_4", Opcodes.ICONST_4);
        ZERO_OPS.put("ICONST_5", Opcodes.ICONST_5);
        ZERO_OPS.put("LCONST_0", Opcodes.LCONST_0);
        ZERO_OPS.put("LCONST_1", Opcodes.LCONST_1);
        ZERO_OPS.put("FCONST_0", Opcodes.FCONST_0);
        ZERO_OPS.put("FCONST_1", Opcodes.FCONST_1);
        ZERO_OPS.put("FCONST_2", Opcodes.FCONST_2);
        ZERO_OPS.put("DCONST_0", Opcodes.DCONST_0);
        ZERO_OPS.put("DCONST_1", Opcodes.DCONST_1);
        ZERO_OPS.put("IALOAD", Opcodes.IALOAD);
        ZERO_OPS.put("LALOAD", Opcodes.LALOAD);
        ZERO_OPS.put("FALOAD", Opcodes.FALOAD);
        ZERO_OPS.put("DALOAD", Opcodes.DALOAD);
        ZERO_OPS.put("AALOAD", Opcodes.AALOAD);
        ZERO_OPS.put("BALOAD", Opcodes.BALOAD);
        ZERO_OPS.put("CALOAD", Opcodes.CALOAD);
        ZERO_OPS.put("SALOAD", Opcodes.SALOAD);
        ZERO_OPS.put("IASTORE", Opcodes.IASTORE);
        ZERO_OPS.put("LASTORE", Opcodes.LASTORE);
        ZERO_OPS.put("FASTORE", Opcodes.FASTORE);
        ZERO_OPS.put("DASTORE", Opcodes.DASTORE);
        ZERO_OPS.put("AASTORE", Opcodes.AASTORE);
        ZERO_OPS.put("BASTORE", Opcodes.BASTORE);
        ZERO_OPS.put("CASTORE", Opcodes.CASTORE);
        ZERO_OPS.put("SASTORE", Opcodes.SASTORE);
        ZERO_OPS.put("POP", Opcodes.POP);
        ZERO_OPS.put("POP2", Opcodes.POP2);
        ZERO_OPS.put("DUP", Opcodes.DUP);
        ZERO_OPS.put("DUP_X1", Opcodes.DUP_X1);
        ZERO_OPS.put("DUP_X2", Opcodes.DUP_X2);
        ZERO_OPS.put("DUP2", Opcodes.DUP2);
        ZERO_OPS.put("DUP2_X1", Opcodes.DUP2_X1);
        ZERO_OPS.put("DUP2_X2", Opcodes.DUP2_X2);
        ZERO_OPS.put("SWAP", Opcodes.SWAP);
        ZERO_OPS.put("IADD", Opcodes.IADD);
        ZERO_OPS.put("LADD", Opcodes.LADD);
        ZERO_OPS.put("FADD", Opcodes.FADD);
        ZERO_OPS.put("DADD", Opcodes.DADD);
        ZERO_OPS.put("ISUB", Opcodes.ISUB);
        ZERO_OPS.put("LSUB", Opcodes.LSUB);
        ZERO_OPS.put("FSUB", Opcodes.FSUB);
        ZERO_OPS.put("DSUB", Opcodes.DSUB);
        ZERO_OPS.put("IMUL", Opcodes.IMUL);
        ZERO_OPS.put("LMUL", Opcodes.LMUL);
        ZERO_OPS.put("FMUL", Opcodes.FMUL);
        ZERO_OPS.put("DMUL", Opcodes.DMUL);
        ZERO_OPS.put("IDIV", Opcodes.IDIV);
        ZERO_OPS.put("LDIV", Opcodes.LDIV);
        ZERO_OPS.put("FDIV", Opcodes.FDIV);
        ZERO_OPS.put("DDIV", Opcodes.DDIV);
        ZERO_OPS.put("IREM", Opcodes.IREM);
        ZERO_OPS.put("LREM", Opcodes.LREM);
        ZERO_OPS.put("FREM", Opcodes.FREM);
        ZERO_OPS.put("DREM", Opcodes.DREM);
        ZERO_OPS.put("INEG", Opcodes.INEG);
        ZERO_OPS.put("LNEG", Opcodes.LNEG);
        ZERO_OPS.put("FNEG", Opcodes.FNEG);
        ZERO_OPS.put("DNEG", Opcodes.DNEG);
        ZERO_OPS.put("ISHL", Opcodes.ISHL);
        ZERO_OPS.put("LSHL", Opcodes.LSHL);
        ZERO_OPS.put("ISHR", Opcodes.ISHR);
        ZERO_OPS.put("LSHR", Opcodes.LSHR);
        ZERO_OPS.put("IUSHR", Opcodes.IUSHR);
        ZERO_OPS.put("LUSHR", Opcodes.LUSHR);
        ZERO_OPS.put("IAND", Opcodes.IAND);
        ZERO_OPS.put("LAND", Opcodes.LAND);
        ZERO_OPS.put("IOR", Opcodes.IOR);
        ZERO_OPS.put("LOR", Opcodes.LOR);
        ZERO_OPS.put("IXOR", Opcodes.IXOR);
        ZERO_OPS.put("LXOR", Opcodes.LXOR);
        ZERO_OPS.put("I2L", Opcodes.I2L);
        ZERO_OPS.put("I2F", Opcodes.I2F);
        ZERO_OPS.put("I2D", Opcodes.I2D);
        ZERO_OPS.put("L2I", Opcodes.L2I);
        ZERO_OPS.put("L2F", Opcodes.L2F);
        ZERO_OPS.put("L2D", Opcodes.L2D);
        ZERO_OPS.put("F2I", Opcodes.F2I);
        ZERO_OPS.put("F2L", Opcodes.F2L);
        ZERO_OPS.put("F2D", Opcodes.F2D);
        ZERO_OPS.put("D2I", Opcodes.D2I);
        ZERO_OPS.put("D2L", Opcodes.D2L);
        ZERO_OPS.put("D2F", Opcodes.D2F);
        ZERO_OPS.put("I2B", Opcodes.I2B);
        ZERO_OPS.put("I2C", Opcodes.I2C);
        ZERO_OPS.put("I2S", Opcodes.I2S);
        ZERO_OPS.put("LCMP", Opcodes.LCMP);
        ZERO_OPS.put("FCMPL", Opcodes.FCMPL);
        ZERO_OPS.put("FCMPG", Opcodes.FCMPG);
        ZERO_OPS.put("DCMPL", Opcodes.DCMPL);
        ZERO_OPS.put("DCMPG", Opcodes.DCMPG);
        ZERO_OPS.put("IRETURN", Opcodes.IRETURN);
        ZERO_OPS.put("LRETURN", Opcodes.LRETURN);
        ZERO_OPS.put("FRETURN", Opcodes.FRETURN);
        ZERO_OPS.put("DRETURN", Opcodes.DRETURN);
        ZERO_OPS.put("ARETURN", Opcodes.ARETURN);
        ZERO_OPS.put("RETURN", Opcodes.RETURN);
        ZERO_OPS.put("ARRAYLENGTH", Opcodes.ARRAYLENGTH);
        ZERO_OPS.put("ATHROW", Opcodes.ATHROW);
        ZERO_OPS.put("MONITORENTER", Opcodes.MONITORENTER);
        ZERO_OPS.put("MONITOREXIT", Opcodes.MONITOREXIT);
    }
}
