//! Build-time identity: which engine this binary carries, and a Windows
//! VERSIONINFO resource so the OS can name the build too.
//!
//! Both exist because of one incident: a service crash showed up in the
//! Windows event log as `dxpdf-service.exe, version: 0.0.0.0` — a PE without
//! VERSIONINFO reports zeros — and the log file said nothing about which
//! dxpdf was inside, so a crash report could not be tied to a build.

use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=DXPDF_ENGINE={}", engine_identity());
    embed_windows_version_resource();
}

/// The dxpdf build this binary links, read from `Cargo.lock` — the only file
/// that knows the resolved commit behind a git dependency. `Cargo.toml` names
/// a tag, and a tag can be moved; the lock records the SHA it resolved to.
fn engine_identity() -> String {
    let Ok(lock) = fs::read_to_string("Cargo.lock") else {
        return "unknown (no Cargo.lock at build time)".to_string();
    };
    let Some((version, source)) = lock_entry(&lock, "dxpdf") else {
        return "unknown (dxpdf not in Cargo.lock)".to_string();
    };
    match source {
        // git+https://host/owner/repo?tag=NAME#SHA
        Some(src) if src.starts_with("git+") => {
            let sha = src.rsplit_once('#').map(|(_, sha)| sha).unwrap_or("");
            let short: String = sha.chars().take(7).collect();
            let reference = ["tag=", "branch=", "rev="]
                .iter()
                .find_map(|key| {
                    let rest = src.split(key).nth(1)?;
                    let value = rest.split('#').next().unwrap_or(rest);
                    Some(format!("{key}{value}"))
                })
                .unwrap_or_else(|| "default branch".to_string());
            let repo = src
                .trim_start_matches("git+")
                .split(['?', '#'])
                .next()
                .unwrap_or("")
                .to_string();
            format!("{version} ({repo} {reference} @ {short})")
        }
        Some(src) => format!("{version} ({src})"),
        // No `source` key means a path dependency: a local checkout, which
        // can be anything, so say so rather than implying a released build.
        None => format!("{version} (local path checkout — not a pinned build)"),
    }
}

/// `(version, source)` of one `[[package]]` entry in a `Cargo.lock`.
fn lock_entry(lock: &str, package: &str) -> Option<(String, Option<String>)> {
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() != format!("name = \"{package}\"") {
            continue;
        }
        let mut version = None;
        let mut source = None;
        for line in lines.by_ref() {
            let trimmed = line.trim();
            // A blank line or the next header ends this entry.
            if trimmed.is_empty() || trimmed == "[[package]]" {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("version = ") {
                version = Some(value.trim_matches('"').to_string());
            } else if let Some(value) = trimmed.strip_prefix("source = ") {
                source = Some(value.trim_matches('"').to_string());
            }
        }
        return version.map(|v| (v, source));
    }
    None
}

/// Stamps FileVersion/ProductVersion and the descriptive strings into the PE,
/// which is what Windows shows in Explorer's properties and — the reason this
/// exists — in an Application Error event's `version:` field.
#[cfg(windows)]
fn embed_windows_version_resource() {
    let mut resource = winresource::WindowsResource::new();
    resource.set("ProductName", "dxpdf-service");
    resource.set(
        "FileDescription",
        "dxpdf DOCX to PDF conversion service (HTTP)",
    );
    resource.set("OriginalFilename", "dxpdf-service.exe");
    resource.set("InternalName", "dxpdf-service");
    resource.set("LegalCopyright", "MIT licensed");
    // The engine is not a version *number*, so it rides in a free-form field
    // rather than in ProductVersion, which Windows parses as four integers.
    resource.set("Comments", &format!("engine: dxpdf {}", engine_identity()));
    if let Err(e) = resource.compile() {
        // Not fatal: a build without the resource still runs, it just reports
        // 0.0.0.0 to the event log again — so say loudly what was lost.
        println!(
            "cargo:warning=VERSIONINFO not embedded ({e}); crash reports will \
             show version 0.0.0.0. Needs the Windows SDK's rc.exe on PATH."
        );
    }
}

#[cfg(not(windows))]
fn embed_windows_version_resource() {
    // Only PE files carry VERSIONINFO, so there is nothing to embed here.
}
