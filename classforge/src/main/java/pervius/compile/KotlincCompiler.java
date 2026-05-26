package pervius.compile;

import org.jetbrains.kotlin.cli.common.ExitCode;
import org.jetbrains.kotlin.cli.common.arguments.K2JVMCompilerArguments;
import org.jetbrains.kotlin.cli.common.messages.CompilerMessageSeverity;
import org.jetbrains.kotlin.cli.common.messages.CompilerMessageSourceLocation;
import org.jetbrains.kotlin.cli.common.messages.MessageCollector;
import org.jetbrains.kotlin.cli.jvm.K2JVMCompiler;
import org.jetbrains.kotlin.config.Services;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileVisitResult;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.SimpleFileVisitor;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Kotlin source compiler bridge backed by kotlin-compiler-embeddable.
 *
 * This class intentionally contains all org.jetbrains.kotlin.* references so
 * ClassForge can keep the Kotlin compiler off the normal -jar path.
 *
 * @author TheFloodDragon
 */
public final class KotlincCompiler {
    private static final int STATUS_OK = 0;
    private static final int STATUS_DIAGNOSTICS = 1;

    private KotlincCompiler() {
    }

    public static byte[] run(String[] args) throws IOException {
        Args parsed = Args.parse(args);
        Path workDir = Files.createTempDirectory("pervius-kt-");
        Path srcDir = Files.createDirectories(workDir.resolve("src"));
        Path outDir = Files.createDirectories(workDir.resolve("out"));
        try {
            List<String> sourceFiles = readSources(System.in, srcDir);
            Collector collector = new Collector();

            K2JVMCompilerArguments compilerArgs = new K2JVMCompilerArguments();
            compilerArgs.setFreeArgs(sourceFiles);
            compilerArgs.setDestination(outDir.toString());
            if (parsed.classpath != null) {
                compilerArgs.setClasspath(parsed.classpath);
            }
            compilerArgs.setModuleName(parsed.moduleName);
            compilerArgs.setJvmTarget(parsed.jvmTarget);
            if (parsed.apiVersion != null) {
                compilerArgs.setApiVersion(parsed.apiVersion);
            }
            if (parsed.languageVersion != null) {
                compilerArgs.setLanguageVersion(parsed.languageVersion);
            }
            compilerArgs.setNoStdlib(true);
            compilerArgs.setNoReflect(true);
            compilerArgs.setSuppressWarnings(false);
            compilerArgs.setSkipMetadataVersionCheck(parsed.skipMetadataVersionCheck);

            ExitCode exit = invokeCompiler(collector, compilerArgs);
            if (exit == ExitCode.OK) {
                return success(outDir);
            }
            return diagnostics(collector.list);
        } finally {
            deleteRecursively(workDir);
        }
    }

    private static ExitCode invokeCompiler(Collector collector, K2JVMCompilerArguments compilerArgs) throws IOException {
        K2JVMCompiler compiler = new K2JVMCompiler();
        for (String methodName : new String[]{"exec", "execImpl"}) {
            for (Method method : compiler.getClass().getMethods()) {
                if (!method.getName().equals(methodName)) {
                    continue;
                }
                Class<?>[] params = method.getParameterTypes();
                if (params.length != 3) {
                    continue;
                }
                if (!params[0].isInstance(collector)
                        || !params[1].isInstance(Services.EMPTY)
                        || !params[2].isAssignableFrom(compilerArgs.getClass())) {
                    continue;
                }
                try {
                    Object result = method.invoke(compiler, collector, Services.EMPTY, compilerArgs);
                    if (result instanceof ExitCode) {
                        return (ExitCode) result;
                    }
                } catch (ReflectiveOperationException e) {
                    throw new IOException("failed to invoke Kotlin compiler entrypoint " + methodName, e);
                }
            }
        }
        throw new IOException("no compatible Kotlin compiler entrypoint found");
    }

    private static List<String> readSources(java.io.InputStream in, Path srcDir) throws IOException {
        DataInputStream dis = new DataInputStream(in);
        int count = dis.readInt();
        if (count <= 0) {
            return Collections.emptyList();
        }
        List<String> paths = new ArrayList<String>(count);
        for (int i = 0; i < count; i++) {
            String relative = readPrefixedString(dis);
            String source = readPrefixedStringU32(dis);
            Path out = safeResolve(srcDir, relative);
            Files.createDirectories(out.getParent());
            Files.write(out, source.getBytes(StandardCharsets.UTF_8));
            paths.add(out.toString());
        }
        return paths;
    }

    private static Path safeResolve(Path root, String relative) throws IOException {
        String normalizedName = relative.replace('\\', '/');
        while (normalizedName.startsWith("/")) {
            normalizedName = normalizedName.substring(1);
        }
        Path resolved = root.resolve(normalizedName).normalize();
        if (!resolved.startsWith(root)) {
            throw new IOException("source path escapes temp dir: " + relative);
        }
        if (!resolved.getFileName().toString().endsWith(".kt")
                && !resolved.getFileName().toString().endsWith(".kts")) {
            resolved = resolved.resolveSibling(resolved.getFileName().toString() + ".kt");
        }
        return resolved;
    }

    private static byte[] success(Path outDir) throws IOException {
        final Map<String, byte[]> classes = new LinkedHashMap<String, byte[]>();
        Files.walkFileTree(outDir, new SimpleFileVisitor<Path>() {
            @Override
            public FileVisitResult visitFile(Path file, BasicFileAttributes attrs) throws IOException {
                if (file.getFileName().toString().endsWith(".class")) {
                    String rel = outDir.relativize(file).toString().replace('\\', '/');
                    String binaryName = rel.substring(0, rel.length() - ".class".length());
                    classes.put(binaryName, Files.readAllBytes(file));
                }
                return FileVisitResult.CONTINUE;
            }
        });

        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        dos.writeByte(STATUS_OK);
        dos.writeInt(classes.size());
        for (Map.Entry<String, byte[]> entry : classes.entrySet()) {
            writePrefixedString(dos, entry.getKey());
            byte[] bytes = entry.getValue();
            dos.writeInt(bytes.length);
            dos.write(bytes);
        }
        dos.flush();
        return baos.toByteArray();
    }

    private static byte[] diagnostics(List<Diag> diagnostics) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        dos.writeByte(STATUS_DIAGNOSTICS);
        dos.writeInt(diagnostics.size());
        for (Diag diagnostic : diagnostics) {
            dos.writeByte(severity(diagnostic.severity));
            dos.writeInt(diagnostic.line());
            dos.writeInt(diagnostic.column());
            writePrefixedStringU32(dos, diagnostic.message);
        }
        dos.flush();
        return baos.toByteArray();
    }

    private static byte severity(CompilerMessageSeverity severity) {
        if (severity.isError()) return 0;
        if (severity == CompilerMessageSeverity.WARNING
                || severity == CompilerMessageSeverity.STRONG_WARNING) {
            return 1;
        }
        return 2;
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

    private static void writePrefixedString(DataOutputStream dos, String s) throws IOException {
        byte[] bytes = s.getBytes(StandardCharsets.UTF_8);
        dos.writeShort(bytes.length);
        dos.write(bytes);
    }

    private static void writePrefixedStringU32(DataOutputStream dos, String s) throws IOException {
        byte[] bytes = s.getBytes(StandardCharsets.UTF_8);
        dos.writeInt(bytes.length);
        dos.write(bytes);
    }

    private static void deleteRecursively(Path root) throws IOException {
        if (root == null || !Files.exists(root)) {
            return;
        }
        Files.walkFileTree(root, new SimpleFileVisitor<Path>() {
            @Override
            public FileVisitResult visitFile(Path file, BasicFileAttributes attrs) throws IOException {
                Files.deleteIfExists(file);
                return FileVisitResult.CONTINUE;
            }

            @Override
            public FileVisitResult postVisitDirectory(Path dir, IOException exc) throws IOException {
                if (exc != null) {
                    throw exc;
                }
                Files.deleteIfExists(dir);
                return FileVisitResult.CONTINUE;
            }
        });
    }

    private static final class Collector implements MessageCollector {
        final List<Diag> list = new ArrayList<Diag>();

        @Override
        public void clear() {
            list.clear();
        }

        @Override
        public boolean hasErrors() {
            for (Diag diag : list) {
                if (diag.severity.isError()) {
                    return true;
                }
            }
            return false;
        }

        @Override
        public void report(
                CompilerMessageSeverity severity,
                String message,
                CompilerMessageSourceLocation location
        ) {
            list.add(new Diag(severity, message, location));
        }
    }

    private static final class Diag {
        final CompilerMessageSeverity severity;
        final String message;
        final CompilerMessageSourceLocation location;

        Diag(CompilerMessageSeverity severity, String message, CompilerMessageSourceLocation location) {
            this.severity = severity;
            this.message = message;
            this.location = location;
        }

        int line() {
            return location == null ? 0 : Math.max(0, location.getLine());
        }

        int column() {
            return location == null ? 0 : Math.max(0, location.getColumn());
        }
    }

    private static final class Args {
        String classpath;
        String jvmTarget = "1.8";
        String moduleName = "pervius";
        String apiVersion;
        String languageVersion;
        boolean skipMetadataVersionCheck = true;

        static Args parse(String[] args) {
            Args parsed = new Args();
            for (int i = 0; i < args.length; i++) {
                if ("--classpath".equals(args[i]) && i + 1 < args.length) {
                    parsed.classpath = args[++i];
                } else if ("--target".equals(args[i]) && i + 1 < args.length) {
                    parsed.jvmTarget = normalizeTarget(args[++i]);
                } else if ("--module-name".equals(args[i]) && i + 1 < args.length) {
                    parsed.moduleName = args[++i];
                } else if ("--api-version".equals(args[i]) && i + 1 < args.length) {
                    parsed.apiVersion = args[++i];
                } else if ("--language-version".equals(args[i]) && i + 1 < args.length) {
                    parsed.languageVersion = args[++i];
                } else if ("--no-skip-metadata-version-check".equals(args[i])) {
                    parsed.skipMetadataVersionCheck = false;
                }
            }
            return parsed;
        }

        private static String normalizeTarget(String target) {
            if ("8".equals(target)) return "1.8";
            return target;
        }
    }
}
