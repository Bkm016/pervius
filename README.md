<div align="center">

<img src="crates/egui-shell/logo.svg" width="84" alt="Pervius logo" />

# Pervius

**Modern Java / Kotlin decompiler, source recompiler, and bytecode editor.**

[Vineflower](https://github.com/Vineflower/vineflower) decompilation · [ClassForge](classforge/) bytecode rewriting · Native Rust UI

[![Rust](https://img.shields.io/badge/Rust-2024_Edition-f74c00?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![egui](https://img.shields.io/badge/egui-0.34-1ba7f5)](https://github.com/emilk/egui)
[![Platform](https://img.shields.io/badge/Platform-Windows_·_macOS_·_Linux-8957e5)](#requirements)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)</br>
[![Decompiler](https://img.shields.io/badge/Decompiler-Vineflower_1.12.0-e76f00?logo=openjdk&logoColor=white)](https://github.com/Vineflower/vineflower)
[![Assembler](https://img.shields.io/badge/Assembler-ClassForge_1.1-b07219)](classforge/)

[Features](#features) · [Requirements](#requirements) · [Build](#build) · [Shortcuts](#shortcuts) · [中文](README_CN.md)

</div>

## Features

### Decompilation

Powered by Vineflower, with both batch JAR decompilation and on-demand single-class decompilation. Small JARs are fully decompiled upfront; large JARs decompile class-by-class on demand. Vineflower progress is parsed in real time and tracked per class. Results are cached by the JAR's SHA-256, so reopening never triggers a rebuild. Kotlin classes can be emitted either as `.kt` (Vineflower/Kotlin output) or as `.java` (Java output mode), with original line-number mapping preserved.

<img src="screenshots/1.png" width="600" alt="Screenshot" />

### Bytecode Editing

Structured `.class` editor: the left pane navigates class info, fields, and methods; the right pane provides the matching editor. Access flags, inheritance, annotations, and descriptors are all editable, along with method instructions. On save, ClassForge (built on ASM 9.7.1) handles constant-pool rebuilding, StackMapTable recomputation, and `max_stack` / `max_locals`. Untouched methods are byte-copied; only modified methods trigger frame recomputation.

<img src="screenshots/3.png" width="600" alt="Screenshot" />

### Source Recompilation

Decompiled Java / Kotlin sources can be unlocked from the code view context menu (`Right Click` → **Allow Editing**). `Ctrl+S` or **Recompile Now** compiles the edited source asynchronously and replaces the generated `.class` entries in the in-memory JAR. Java recompilation uses the JDK `javax.tools.JavaCompiler`; Kotlin recompilation uses `kotlin-compiler-embeddable` on a dedicated `-cp` launch path so the normal ClassForge modes do not load the Kotlin compiler. Compiler diagnostics are returned to the editor and shown as gutter markers without blocking further edits.

Source editing is mutually exclusive with the structured bytecode editor: save or discard one path before switching to the other.

### Tri-View

Every `.class` can be toggled between three views with `Tab`:

- **Decompiled view** — syntax-highlighted Java / Kotlin source, read-only by default and unlockable for source recompilation
- **Bytecode view** — structured editor
- **Hex view** — interactive hex inspector

Non-`.class` text files (XML, YAML, JSON, etc.) are editable directly with syntax highlighting; binary files open in the hex view.

<img src="screenshots/4.png" width="600" alt="Screenshot" />

### Code Navigation

`Ctrl+Click` (macOS `Cmd+Click`) jumps to class, method, or field definitions. Supports import resolution, same-package inference, wildcard matching, and Kotlin/JVM special names such as backtick-quoted or `$` methods. `Ctrl+Click` on a method declaration triggers Find Usages, searching all references automatically.

### Global Search

`Double Shift` opens the search panel across every decompiled source, with regex and case-sensitivity support. Results stream back grouped by class with line-level highlighted previews; double-click to jump. The index is built in the background after decompilation completes and never blocks the UI.

<img src="screenshots/2.png" width="600" alt="Screenshot" />

### Archive Browsing

The left-hand resource tree lists JAR contents and supports `jar`, `zip`, `war`, and `ear`. Type to filter (Speed Search) with filtering computed on a background thread. Modified and decompilation states are reflected in real time. Dropping a file onto the window opens the first dropped archive or standalone file. The Classpath panel is shown directly inside the explorer, accepts archive/directory additions through its `+` action, and its height can be resized by dragging the top edge. Recent files are also tracked.

<img src="screenshots/5.png" width="600" alt="Screenshot" />

### Export

- **Save / overwrite source JAR** (`Ctrl+S`) — writes in-memory modified entries back to the currently opened archive when no editable source tab is pending
- **Export JAR** (`Ctrl+Shift+S`) — writes modifications back and produces a new archive
- **Export decompiled sources** (`Ctrl+Shift+E`) — exports `.java` / `.kt` to a directory, preserving the package layout

## Requirements

- A working Java runtime is required for decompilation / ClassForge execution; Pervius can use the Java path configured in Settings, `JAVA_HOME`, or `java` from `PATH`
- A **JDK** (not just a JRE) is required for Java and Kotlin source recompilation, because ClassForge calls the system `javac`
- Vineflower and Kotlin compiler dependencies are downloaded automatically from the Huawei Cloud Maven mirror into the Environment tools directory (by default under the decompile cache root)

ClassForge is bundled and extracted to the data directory on first launch. Vineflower is resolved from the configured Environment directory and downloaded on demand; a matching `vineflower-{version}.jar` next to the executable still takes priority for local/offline override. Kotlin dependencies (`kotlin-stdlib` and `kotlin-compiler-embeddable`) are intentionally not bundled to keep the default distribution small and are downloaded only when Kotlin source recompilation is used. Download progress is surfaced in the status bar, and non-JAR Maven artifacts declared only as POM metadata are skipped automatically during dependency resolution.

## Build

```bash
cargo build --release
```

ClassForge is embedded via `include_bytes!`; Vineflower and Kotlin dependencies are resolved by the Environment settings and downloaded on demand, with progress shown in the status bar.

Build ClassForge (only required after modifying ClassForge sources):

```bash
cd classforge
./gradlew jar    # Windows: .\gradlew.bat jar
```

ClassForge declares Kotlin dependencies as `compileOnly`: Gradle / javac can type-check `KotlincCompiler`, but Kotlin stdlib/compiler are not packed into `classforge-*.jar`. Copy the resulting `classforge-1.1.jar` into `crates/pervius-java-bridge/libs/`, replacing the bundled JAR if ClassForge sources change, then rebuild Rust. Keep the default Kotlin version in `classforge/build.gradle` and `crates/pervius-java-bridge/src/environment.rs` in sync; runtime Kotlin recompilation downloads the configured Kotlin dependencies automatically.

```bash
cargo run --release
```

## Shortcuts

| Shortcut | Action |
|:---------|:-------|
| `Ctrl+O` | Open archive or single file |
| `Ctrl+S` | Save source edit / overwrite current JAR |
| `Ctrl+F` | Find |
| `Double Shift` | Global search |
| `Ctrl+Click` | Go to definition / Find Usages |
| `Tab` | Switch view |
| `Alt+1` | Toggle resource tree |
| `Ctrl+Shift+S` | Export JAR |
| `Ctrl+Shift+E` | Export decompiled sources |
| `Ctrl+,` | Settings |

All shortcuts can be customized in Settings.

## Credits

- [Vineflower](https://github.com/Vineflower/vineflower) — Java decompilation engine
- [ASM](https://asm.ow2.io/) — Java bytecode manipulation framework
- [egui](https://github.com/emilk/egui) — Rust immediate mode GUI
- [tree-sitter](https://tree-sitter.github.io/tree-sitter/) — syntax highlighting

## License

[MIT](LICENSE)
