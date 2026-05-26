# pervius-java-bridge

Pervius 的 Java 工具链桥接层，封装 JAR/ZIP 归档读写、Vineflower 反编译、JVM 字节码反汇编、源码重编译、ClassForge 字节码写回和外部工具依赖管理。纯 Rust 实现，无 UI 依赖。

## 模块

| 模块 | 职责 |
|------|------|
| `jar` | JAR/ZIP 归档读取、SHA-256 哈希、条目内存管理 |
| `bytecode` | `.class` 字节 → 结构化 `ClassStructure`（Recaf 风格反汇编） |
| `class_structure` | 纯数据结构定义（类/字段/方法/注解） |
| `decompiler` | Vineflower 子进程管理、Kotlin/Java 输出模式、磁盘缓存、进度追踪 |
| `assembler` | ClassForge (ASM) IPC，字节码重组装 |
| `compiler` | Java / Kotlin 源码重编译入口 |
| `environment` | Vineflower / Kotlin 依赖配置、下载进度与默认目录解析 |
| `deps` | Maven 依赖下载、校验与 POM 解析（内部模块） |
| `process` | Java 子进程构建器（定位 java、管道配置、进程树终止） |
| `save` | `ClassStructure` 编辑写回 class 字节 |
| `error` | `BridgeError` 统一错误类型 |

## 用法

### 打开 JAR

```rust
use pervius_java_bridge::jar::{JarArchive, LoadProgress};

let progress = LoadProgress::new();
let jar = JarArchive::open_with_progress(Path::new("server.jar"), &progress)?;

for path in jar.paths() {
    println!("{path}");
}

let bytes = jar.get("com/example/Main.class").unwrap();
```

`LoadProgress` 的 `current` / `total` 是 `AtomicU32`，可从 UI 线程轮询加载进度。JAR 打开时计算 SHA-256 哈希（`jar.hash`），前 16 字符用作反编译缓存目录名。

### 反汇编

```rust
use pervius_java_bridge::bytecode;

let bytes = jar.get("com/example/PlayerManager.class").unwrap();
let cs = bytecode::disassemble(bytes)?;

println!("{} {} extends {}", cs.info.access, cs.info.name, cs.info.super_class);
for method in &cs.methods {
    println!("{} {}{}", method.access, method.name, method.descriptor);
    println!("{}", method.bytecode);
}
```

反汇编输出采用 Recaf 风格：操作码大写，常量池引用内联解析为可读名称，分支目标用字母标签，变量槽位解析为名称。

### 反编译

```rust
use pervius_java_bridge::{decompiler, environment};

// 启动前同步可选环境配置；空目录表示使用默认依赖目录。
environment::set_environment_config(environment::EnvironmentConfig::default());
decompiler::set_kotlin_decompiler_mode(decompiler::KotlinDecompilerMode::Vineflower);

if decompiler::is_cached(&jar.hash) {
    if let Some(src) = decompiler::cached_source(&jar.hash, "com/example/Main.class") {
        println!("{}", src.source);
        println!("language: {:?}", src.language);
    }
}

let task = decompiler::start(&jar.path, &jar.name, &jar.hash, jar.class_count())?;
let current = task.progress.current.load(std::sync::atomic::Ordering::Relaxed);
let total = task.progress.total.load(std::sync::atomic::Ordering::Relaxed);
println!("Decompiling {current}/{total}");
```

Vineflower 从 `environment` 配置目录解析并按需下载；可执行文件同目录下匹配版本的 `vineflower-{version}.jar` 仍优先用于本地/离线覆盖。缓存写入当前缓存根目录下按 Kotlin 输出模式区分的子目录：

- `decompiled/kotlin/<hash_prefix>/` — Vineflower Kotlin 输出模式
- `decompiled/java/<hash_prefix>/` — Java 输出模式

`DecompileTask` drop 时会自动 kill 子进程（Windows 用 `taskkill /F /T`，Unix 用 `kill`）。

单文件反编译用于保存编辑后立即更新预览：

```rust
let new_bytes = save::apply_structure(raw, &edited_cs, Some(&jar.path))?;
let src = decompiler::decompile_single_class(
    &new_bytes,
    "com/example/Main.class",
    Some(&jar.path),
    Some(&jar.name),
    None, // 不写入缓存
)?;
```

### 源码重编译

```rust
use pervius_java_bridge::compiler::{self, CompileClasspath, KotlinSource};

let classpath = CompileClasspath::default();
let java = compiler::compile_source(
    "package com.example; public class Main {}",
    "com/example/Main",
    &classpath,
    Some(17),
    true,
)?;

let kotlin = compiler::compile_kotlin_sources_with_options(
    &[KotlinSource {
        path: "com/example/Main.kt".into(),
        source: "package com.example\nclass Main".into(),
    }],
    &classpath,
    Some(17),
    None,
    true,
)?;
```

Java 重编译通过 ClassForge 调用系统 `javac`；Kotlin 重编译会按需下载 `kotlin-stdlib`、`kotlin-compiler-embeddable` 及其运行时依赖，并用独立 `-cp` 启动路径加载编译器。

### 编辑写回

```rust
use pervius_java_bridge::save;

cs.methods[0].bytecode = "ICONST_1\nIRETURN".to_string();
cs.info.name = "com/example/RenamedClass".to_string();
cs.fields[0].access = "public static final".to_string();

let new_bytes = save::apply_structure(raw_bytes, &cs, Some(&jar.path))?;
jar.put("com/example/Main.class", new_bytes);
```

写回分两步：

1. **ristretto 处理 metadata** — 类名、字段名、方法名、描述符、访问标志、注解写回常量池
2. **ClassForge (ASM) 处理字节码** — 将 Pervius 格式的字节码文本解析为 JVM 指令，重建常量池引用、生成 StackMapTable、计算 `max_stack` / `max_locals`

ClassForge 通过 stdin 二进制协议通信（大端序）：

```text
[4B class 长度][class 字节][4B 编辑数]
每条编辑: [2B name 长度][name][2B desc 长度][desc][4B code 长度][code 文本]
```

### 工具函数

```rust
use pervius_java_bridge::{assembler, environment};

// ClassForge 内置到应用数据目录；exe 同目录同名前缀 JAR 可覆盖。
let classforge_version = assembler::classforge_version();

// Vineflower 由环境配置定位并按需下载。
let vineflower = environment::ensure_vineflower()?;
```

### 错误处理

所有可失败操作返回 `Result<T, BridgeError>`。`BridgeError` 实现 `Display`、`Error`、`From<io::Error>`、`Send`，调用方可用 `?` 传播、`{e}` 格式化、跨线程 `mpsc::channel` 传递。

## 依赖

| crate | 用途 |
|-------|------|
| `ristretto_classfile` | JVM class 文件解析与序列化 |
| `zip` | ZIP/JAR 归档读取 |
| `sha2` | SHA-256 哈希（缓存键） |
| `dirs` | 系统缓存 / 数据目录定位 |
| `serde` / `toml` | 缓存元数据与配置序列化 |
| `ureq` | Maven 工件下载 |
| `log` | 日志输出 |
