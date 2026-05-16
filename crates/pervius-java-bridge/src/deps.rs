//! Maven 依赖下载、校验与 POM 解析。
//!
//! @author TheFloodDragon

use crate::environment::{DownloadProgressSnapshot, KotlinDependencies, MAVEN_BASE};
use crate::error::BridgeError;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone, Debug)]
struct ActiveDownload {
    id: u64,
    snapshot: DownloadProgressSnapshot,
}

struct DownloadProgressGuard {
    id: u64,
}

#[derive(Clone, Debug)]
struct PomDependency {
    group_path: String,
    artifact_id: String,
    version: String,
    scope: Option<String>,
    optional: bool,
    allow_transitive: bool,
}

#[derive(Clone, Copy)]
struct MavenJar<'a> {
    group_path: &'a str,
    artifact_id: &'a str,
    version: &'a str,
}

impl MavenJar<'_> {
    fn file_name(self) -> String {
        format!("{}-{}.jar", self.artifact_id, self.version)
    }

    fn pom_file_name(self) -> String {
        format!("{}-{}.pom", self.artifact_id, self.version)
    }

    fn jar_url(self) -> String {
        format!(
            "{}/{}/{}/{}/{}",
            MAVEN_BASE,
            self.group_path,
            self.artifact_id,
            self.version,
            self.file_name()
        )
    }

    fn sha256_url(self) -> String {
        format!("{}.sha256", self.jar_url())
    }

    fn pom_url(self) -> String {
        format!(
            "{}/{}/{}/{}/{}",
            MAVEN_BASE,
            self.group_path,
            self.artifact_id,
            self.version,
            self.pom_file_name()
        )
    }
}

static DOWNLOAD_PROGRESS: Mutex<Option<ActiveDownload>> = Mutex::new(None);
static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

/// 获取当前下载进度快照。
pub(crate) fn download_progress() -> Option<DownloadProgressSnapshot> {
    DOWNLOAD_PROGRESS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
        .map(|active| active.snapshot.clone())
}

/// 在指定目录中定位并按需下载 Vineflower。
pub(crate) fn ensure_vineflower_in_dir(dir: &Path, version: &str) -> Result<PathBuf, BridgeError> {
    resolve_maven_jar(
        dir,
        MavenJar {
            group_path: "org/vineflower",
            artifact_id: "vineflower",
            version,
        },
    )
}

/// 在指定目录中定位并按需下载 Kotlin 编译依赖。
pub(crate) fn ensure_kotlin_dependencies_in_dir(
    dir: &Path,
    version: &str,
) -> Result<KotlinDependencies, BridgeError> {
    let compiler = MavenJar {
        group_path: "org/jetbrains/kotlin",
        artifact_id: "kotlin-compiler-embeddable",
        version,
    };
    let compiler_embeddable = resolve_maven_jar(dir, compiler)?;
    let mut stdlib = None;
    let mut runtime_dependencies = Vec::new();
    let mut seen = HashSet::new();
    collect_dependency_closure(
        dir,
        compiler,
        true,
        &mut seen,
        &mut runtime_dependencies,
        &mut stdlib,
    )?;
    let stdlib = stdlib.ok_or_else(|| {
        BridgeError::Download(format!(
            "kotlin-compiler-embeddable {} pom does not declare kotlin-stdlib runtime dependency",
            version
        ))
    })?;
    Ok(KotlinDependencies {
        stdlib,
        compiler_embeddable,
        runtime_dependencies,
    })
}

fn collect_dependency_closure(
    dir: &Path,
    artifact: MavenJar<'_>,
    runtime_only: bool,
    seen: &mut HashSet<String>,
    out: &mut Vec<PathBuf>,
    stdlib: &mut Option<PathBuf>,
) -> Result<(), BridgeError> {
    for dependency in parse_pom_dependencies(&load_cached_text_file(
        dir,
        &artifact.pom_file_name(),
        &artifact.pom_url(),
    )?)? {
        if dependency.optional || !dependency_scope_matches(dependency.scope.as_deref(), runtime_only) {
            continue;
        }
        let jar = MavenJar {
            group_path: &dependency.group_path,
            artifact_id: &dependency.artifact_id,
            version: &dependency.version,
        };
        let key = format!(
            "{}:{}:{}",
            dependency.group_path, dependency.artifact_id, dependency.version
        );
        if !seen.insert(key) {
            continue;
        }
        if let Some(path) = resolve_optional_maven_jar(dir, jar)? {
            if dependency.artifact_id == "kotlin-stdlib" {
                *stdlib = Some(path.clone());
            }
            out.push(path);
        }
        if dependency.allow_transitive {
            collect_dependency_closure(dir, jar, false, seen, out, stdlib)?;
        }
    }
    Ok(())
}

fn dependency_scope_matches(scope: Option<&str>, runtime_only: bool) -> bool {
    match scope.unwrap_or("compile") {
        "runtime" => true,
        "compile" | "" => !runtime_only,
        _ => false,
    }
}

fn parse_pom_dependencies(xml: &str) -> Result<Vec<PomDependency>, BridgeError> {
    let Some(section) = xml_section(xml, "dependencies") else {
        return Ok(Vec::new());
    };
    let mut dependencies = Vec::new();
    let mut rest = section;
    while let Some(start) = rest.find("<dependency>") {
        let block = &rest[start + "<dependency>".len()..];
        let Some(end) = block.find("</dependency>") else {
            return Err(BridgeError::Download(
                "invalid pom: dependency block is not closed".to_string(),
            ));
        };
        let dependency = &block[..end];
        let group_id = xml_tag_text(dependency, "groupId").ok_or_else(|| {
            BridgeError::Download("invalid pom: dependency.groupId is missing".to_string())
        })?;
        let artifact_id = xml_tag_text(dependency, "artifactId").ok_or_else(|| {
            BridgeError::Download("invalid pom: dependency.artifactId is missing".to_string())
        })?;
        let version = xml_tag_text(dependency, "version").ok_or_else(|| {
            BridgeError::Download("invalid pom: dependency.version is missing".to_string())
        })?;
        let scope = xml_tag_text(dependency, "scope");
        let optional = xml_tag_text(dependency, "optional")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"));
        let allow_transitive = xml_section(dependency, "exclusions").is_none();
        dependencies.push(PomDependency {
            group_path: group_id.replace('.', "/"),
            artifact_id,
            version,
            scope,
            optional,
            allow_transitive,
        });
        rest = &block[end + "</dependency>".len()..];
    }
    Ok(dependencies)
}

fn xml_section<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");
    let start = xml.find(&start_tag)? + start_tag.len();
    let end = xml[start..].find(&end_tag)? + start;
    Some(&xml[start..end])
}

fn xml_tag_text(xml: &str, tag: &str) -> Option<String> {
    xml_section(xml, tag).map(|section| section.trim().to_string())
}

fn resolve_maven_jar(dir: &Path, artifact: MavenJar<'_>) -> Result<PathBuf, BridgeError> {
    let file_name = artifact.file_name();
    if let Some(path) = jar_next_to_exe(&file_name) {
        return Ok(path);
    }
    ensure_verified_download(dir, &file_name, &artifact.jar_url(), &artifact.sha256_url())
}

fn resolve_optional_maven_jar(
    dir: &Path,
    artifact: MavenJar<'_>,
) -> Result<Option<PathBuf>, BridgeError> {
    match resolve_maven_jar(dir, artifact) {
        Ok(path) => Ok(Some(path)),
        Err(BridgeError::Download(message)) if message.contains("status code 404") => {
            if artifact_publishes_jar(dir, artifact)? {
                Err(BridgeError::Download(message))
            } else {
                log::info!(
                    "Skipping non-JAR Maven artifact {}:{}:{}",
                    artifact.group_path,
                    artifact.artifact_id,
                    artifact.version
                );
                Ok(None)
            }
        }
        Err(error) => Err(error),
    }
}

fn artifact_publishes_jar(dir: &Path, artifact: MavenJar<'_>) -> Result<bool, BridgeError> {
    let pom = load_cached_text_file(dir, &artifact.pom_file_name(), &artifact.pom_url())?;
    Ok(xml_tag_text(&pom, "packaging")
        .map(|packaging| packaging.eq_ignore_ascii_case("jar"))
        .unwrap_or(true))
}

fn jar_next_to_exe(file_name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.join(file_name);
    if is_non_empty_file(&path) {
        Some(path)
    } else {
        None
    }
}

fn ensure_verified_download(
    dir: &Path,
    file_name: &str,
    jar_url: &str,
    checksum_url: &str,
) -> Result<PathBuf, BridgeError> {
    std::fs::create_dir_all(dir)?;
    let checksum_file = format!("{file_name}.sha256");
    let expected = fetch_sha256(dir, &checksum_file, checksum_url)?;
    let target = dir.join(file_name);
    if verify_existing_file(&target, &expected)? {
        return Ok(target);
    }
    log::info!("Downloading {file_name} from {jar_url}");
    let tmp = dir.join(format!(".{file_name}.download"));
    cleanup_file(&tmp);
    download_to_file(jar_url, &tmp)?;
    verify_sha256_file(&tmp, &expected, file_name)?;
    if target.exists() {
        cleanup_file(&target);
    }
    std::fs::rename(&tmp, &target)?;
    Ok(target)
}

fn verify_existing_file(path: &Path, expected: &str) -> Result<bool, BridgeError> {
    if !is_non_empty_file(path) {
        return Ok(false);
    }
    match sha256_file(path) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected) => return Ok(true),
        Ok(actual) => {
            log::warn!(
                "Checksum mismatch for {}, expected {expected}, got {actual}; re-downloading",
                path.display()
            );
        }
        Err(error) => {
            log::warn!(
                "Failed to verify existing download {}: {error}; re-downloading",
                path.display()
            );
        }
    }
    cleanup_file(path);
    Ok(false)
}

fn verify_sha256_file(path: &Path, expected: &str, file_name: &str) -> Result<(), BridgeError> {
    let actual = sha256_file(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    cleanup_file(path);
    Err(BridgeError::Download(format!(
        "checksum mismatch for {file_name}: expected {expected}, got {actual}"
    )))
}

fn fetch_sha256(dir: &Path, file_name: &str, url: &str) -> Result<String, BridgeError> {
    let path = dir.join(file_name);
    let text = load_cached_text_file(dir, file_name, url)?;
    if let Some(checksum) = parse_sha256_text(&text) {
        return Ok(checksum);
    }
    cleanup_file(&path);
    let text = load_cached_text_file(dir, file_name, url)?;
    parse_sha256_text(&text).ok_or_else(|| {
        BridgeError::Download(format!("invalid sha256 response from {url}: {text}"))
    })
}

fn parse_sha256_text(text: &str) -> Option<String> {
    let checksum = text
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    (checksum.len() == 64 && checksum.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(checksum)
}

fn load_cached_text_file(dir: &Path, file_name: &str, url: &str) -> Result<String, BridgeError> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file_name);
    if is_non_empty_file(&path) {
        return std::fs::read_to_string(&path).map_err(BridgeError::Io);
    }
    let text = download_text(url)?;
    std::fs::write(&path, &text)?;
    Ok(text)
}

fn download_response(url: &str) -> Result<ureq::Response, BridgeError> {
    ureq::get(url)
        .call()
        .map_err(|e| BridgeError::Download(format!("failed to download {url}: {e}")))
}

fn download_text(url: &str) -> Result<String, BridgeError> {
    download_response(url)?
        .into_string()
        .map_err(|e| BridgeError::Download(format!("failed to read {url}: {e}")))
}

fn download_to_file(url: &str, path: &Path) -> Result<(), BridgeError> {
    let response = download_response(url)?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.trim_start_matches('.').trim_end_matches(".download"))
        .filter(|name| !name.is_empty())
        .unwrap_or("download")
        .to_string();
    let progress = begin_download_progress(file_name, total);
    let mut reader = response.into_reader();
    let mut file = File::create(path)?;
    let mut buf = [0_u8; 64 * 1024];
    let mut downloaded = 0_u64;
    loop {
        let read = reader
            .read(&mut buf)
            .map_err(|e| BridgeError::Download(format!("failed to read {url}: {e}")))?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])?;
        downloaded += read as u64;
        progress.update(downloaded);
    }
    file.flush()?;
    if !is_non_empty_file(path) {
        cleanup_file(path);
        return Err(BridgeError::Download(format!(
            "downloaded file is empty: {}",
            path.display()
        )));
    }
    Ok(())
}

fn begin_download_progress(file_name: String, total: Option<u64>) -> DownloadProgressGuard {
    let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
    let snapshot = DownloadProgressSnapshot {
        file_name,
        downloaded: 0,
        total,
    };
    let mut lock = DOWNLOAD_PROGRESS.lock().unwrap_or_else(|p| p.into_inner());
    *lock = Some(ActiveDownload { id, snapshot });
    DownloadProgressGuard { id }
}

impl DownloadProgressGuard {
    fn update(&self, downloaded: u64) {
        let mut lock = DOWNLOAD_PROGRESS.lock().unwrap_or_else(|p| p.into_inner());
        let Some(active) = lock.as_mut() else {
            return;
        };
        if active.id == self.id {
            active.snapshot.downloaded = downloaded;
        }
    }
}

impl Drop for DownloadProgressGuard {
    fn drop(&mut self) {
        let mut lock = DOWNLOAD_PROGRESS.lock().unwrap_or_else(|p| p.into_inner());
        if lock.as_ref().is_some_and(|active| active.id == self.id) {
            *lock = None;
        }
    }
}

fn sha256_file(path: &Path) -> Result<String, BridgeError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(bytes_to_hex(hasher.finalize().as_slice()))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn cleanup_file(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

fn is_non_empty_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false)
}
