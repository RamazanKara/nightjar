//! Unified release metadata fetching and SHA-256-verified asset downloading shared by
//! Nightjar Android and Nightjar Desktop.

pub mod net;

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nightjar_policy::http::TransferCancel;
use nightjar_policy::update::{ChecksumFile, Release, is_safe_asset_name};
use ring::digest::{Context, SHA256, digest};

use net::{OpenError, Transport, Url, api_host, download_host, open};

const NIGHTJAR_LATEST_URL: &str =
    "https://api.github.com/repos/RamazanKara/nightjar/releases/latest";
const RUBYLIGHT_LATEST_URL: &str =
    "https://api.github.com/repos/RamazanKara/Rubylight/releases/latest";
const MAX_RELEASE_BYTES: u64 = 4 << 20;
const MAX_SUMS_BYTES: u64 = 64 * 1024;

/// Which GitHub repository's `/releases/latest` endpoint to query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateTarget {
    Nightjar = 0,
    Rubylight = 1,
}

impl UpdateTarget {
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Nightjar),
            1 => Some(Self::Rubylight),
            _ => None,
        }
    }

    pub fn latest_url(self) -> &'static str {
        match self {
            Self::Nightjar => NIGHTJAR_LATEST_URL,
            Self::Rubylight => RUBYLIGHT_LATEST_URL,
        }
    }
}

/// Errors returned by release fetching and verified downloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateError {
    RateLimited,
    NoApk,
    NoChecksums,
    ChecksumEntryMissing,
    ChecksumMismatch,
    SizeMismatch,
    Address(String),
    Network(String),
    Io(String),
    Malformed(String),
    Cancelled,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited => write!(f, "GitHub rate limit"),
            Self::NoApk => write!(f, "NO_APK"),
            Self::NoChecksums => write!(f, "NO_CHECKSUMS"),
            Self::ChecksumEntryMissing => write!(f, "CHECKSUM_ENTRY_MISSING"),
            Self::ChecksumMismatch => write!(f, "CHECKSUM_MISMATCH"),
            Self::SizeMismatch => write!(f, "SIZE_MISMATCH"),
            Self::Address(host) => write!(f, "unexpected address: {host}"),
            Self::Network(msg) => write!(f, "{msg}"),
            Self::Io(msg) => write!(f, "{msg}"),
            Self::Malformed(msg) => write!(f, "{msg}"),
            Self::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

impl From<OpenError> for UpdateError {
    fn from(err: OpenError) -> Self {
        match err {
            OpenError::Address(host) => Self::Address(host),
            OpenError::Network(msg) => Self::Network(msg),
        }
    }
}

/// Lowercase hex SHA-256 digest of `bytes` (via `ring::digest::SHA256`).
pub fn sha256_hex(bytes: &[u8]) -> String {
    ChecksumFile::hex(digest(&SHA256, bytes).as_ref())
}

/// Fetches the latest GitHub release for `target`.
pub fn fetch_latest(net: &dyn Transport, target: UpdateTarget) -> Result<Release, UpdateError> {
    fetch_latest_with(net, target.latest_url(), api_host)
}

pub(crate) fn fetch_latest_with(
    net: &dyn Transport,
    url: &str,
    allow: fn(&Url) -> bool,
) -> Result<Release, UpdateError> {
    let response = open(net, url, allow, true)?;
    let status = response.status;
    let mut body = Vec::new();
    response
        .body
        .take(MAX_RELEASE_BYTES)
        .read_to_end(&mut body)
        .map_err(|e| UpdateError::Network(format!("read: {e}")))?;
    match status {
        200 => {
            let text = std::str::from_utf8(&body)
                .map_err(|_| UpdateError::Malformed("the answer is not UTF-8".into()))?;
            Release::parse(text).map_err(UpdateError::Malformed)
        }
        403 | 429 => Err(UpdateError::RateLimited),
        _ => Err(UpdateError::Network(format!("GitHub answered {status}"))),
    }
}

/// Downloads and verifies an asset into `part`, checking exact expected size (when `expected_size`
/// is `Some`) and lowercase hex SHA-256 digest. Cleans up `part` on any error.
#[allow(clippy::too_many_arguments)]
pub fn download_verified_file(
    net: &dyn Transport,
    allow: fn(&Url) -> bool,
    url: &str,
    expected_size: Option<u64>,
    expected_digest: &str,
    part: &Path,
    cancel: Option<&TransferCancel>,
    is_cancelled: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<(), UpdateError> {
    let res = download_verified_inner(
        net,
        allow,
        url,
        expected_size,
        expected_digest,
        part,
        cancel,
        is_cancelled,
        on_progress,
    );
    if res.is_err()
        && let Err(e) = std::fs::remove_file(part)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("nightjar: update: leftover {}: {e}", part.display());
    }
    res
}

#[allow(clippy::too_many_arguments)]
fn download_verified_inner(
    net: &dyn Transport,
    allow: fn(&Url) -> bool,
    url: &str,
    expected_size: Option<u64>,
    expected_digest: &str,
    part: &Path,
    cancel: Option<&TransferCancel>,
    is_cancelled: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<(), UpdateError> {
    let cancelled = || is_cancelled() || cancel.is_some_and(TransferCancel::is_cancelled);
    if cancelled() {
        return Err(UpdateError::Cancelled);
    }
    let mut response = match open(net, url, allow, false) {
        Ok(r) => r,
        Err(_) if cancelled() => return Err(UpdateError::Cancelled),
        Err(e) => return Err(e.into()),
    };
    if cancelled() {
        return Err(UpdateError::Cancelled);
    }
    if response.status != 200 {
        return Err(UpdateError::Network(format!("HTTP {}", response.status)));
    }
    if let Some(expected) = expected_size
        && response.length.is_some_and(|length| length != expected)
    {
        return Err(UpdateError::SizeMismatch);
    }
    let total = response.length.or(expected_size).unwrap_or(0);
    if let Some(dir) = part.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| UpdateError::Io(format!("{}: {e}", part.display())))?;
    }
    let mut file =
        File::create(part).map_err(|e| UpdateError::Io(format!("{}: {e}", part.display())))?;
    let mut hash = Context::new(&SHA256);
    let mut received = 0u64;
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        if cancelled() {
            return Err(UpdateError::Cancelled);
        }
        let read = match response.body.read(&mut chunk) {
            Ok(n) => n,
            Err(_) if cancelled() => return Err(UpdateError::Cancelled),
            Err(e) => return Err(UpdateError::Network(e.to_string())),
        };
        if read == 0 {
            break;
        }
        received += read as u64;
        if let Some(expected) = expected_size
            && received > expected
        {
            return Err(UpdateError::SizeMismatch);
        }
        hash.update(&chunk[..read]);
        file.write_all(&chunk[..read])
            .map_err(|e| UpdateError::Io(format!("{}: {e}", part.display())))?;
        on_progress(received, total);
    }
    if cancelled() {
        return Err(UpdateError::Cancelled);
    }
    if let Some(expected) = expected_size
        && received != expected
    {
        return Err(UpdateError::SizeMismatch);
    }
    let actual = ChecksumFile::hex(hash.finish().as_ref());
    if !actual.eq_ignore_ascii_case(expected_digest) {
        return Err(UpdateError::ChecksumMismatch);
    }
    file.sync_all()
        .map_err(|e| UpdateError::Io(format!("{}: {e}", part.display())))?;
    Ok(())
}

/// Downloads an Android APK checksum-first, streams & verifies SHA-256, and renames `.part` -> target.
pub fn download_android_apk(
    net: &dyn Transport,
    release: &Release,
    root: bool,
    dir: &Path,
    cancel: Option<&TransferCancel>,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, UpdateError> {
    download_android_apk_with(net, download_host, release, root, dir, cancel, on_progress)
}

pub(crate) fn download_android_apk_with(
    net: &dyn Transport,
    allow: fn(&Url) -> bool,
    release: &Release,
    root: bool,
    dir: &Path,
    cancel: Option<&TransferCancel>,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, UpdateError> {
    let apk = release.apk_asset(root).ok_or(UpdateError::NoApk)?;
    if !is_safe_asset_name(&apk.name) {
        return Err(UpdateError::Malformed("unsafe asset name".into()));
    }
    let sums = release.checksum_asset().ok_or(UpdateError::NoChecksums)?;
    if !apk.url.to_ascii_lowercase().starts_with("https://")
        || !sums.url.to_ascii_lowercase().starts_with("https://")
    {
        return Err(UpdateError::Address("HTTPS required".into()));
    }
    let sums_text = fetch_text(net, allow, &sums.url, cancel)?;
    let expected =
        ChecksumFile::find(&sums_text, &apk.name).ok_or(UpdateError::ChecksumEntryMissing)?;

    clean_dir(dir);
    std::fs::create_dir_all(dir)
        .map_err(|e| UpdateError::Io(format!("Cannot create {}: {e}", dir.display())))?;
    let part = dir.join(format!("{}.part", apk.name));
    let target = dir.join(&apk.name);
    let expected_size = u64::try_from(apk.size)
        .ok()
        .filter(|s| *s > 0)
        .ok_or(UpdateError::SizeMismatch)?;
    let mut last_notify = Instant::now() - Duration::from_secs(1);
    download_verified_file(
        net,
        allow,
        &apk.url,
        Some(expected_size),
        &expected,
        &part,
        cancel,
        &|| false,
        &mut |done, total| {
            if last_notify.elapsed() >= Duration::from_millis(50) || (total > 0 && done == total) {
                last_notify = Instant::now();
                on_progress(done, total);
            }
        },
    )?;
    if let Err(e) = std::fs::rename(&part, &target) {
        let _ = std::fs::remove_file(&part);
        return Err(UpdateError::Io(format!(
            "Cannot rename {}: {e}",
            part.display()
        )));
    }
    Ok(target)
}

fn clean_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
}

fn fetch_text(
    net: &dyn Transport,
    allow: fn(&Url) -> bool,
    url: &str,
    cancel: Option<&TransferCancel>,
) -> Result<String, UpdateError> {
    if cancel.is_some_and(TransferCancel::is_cancelled) {
        return Err(UpdateError::Cancelled);
    }
    let response = match open(net, url, allow, false) {
        Ok(r) => r,
        Err(_) if cancel.is_some_and(TransferCancel::is_cancelled) => {
            return Err(UpdateError::Cancelled);
        }
        Err(e) => return Err(e.into()),
    };
    if response.status != 200 {
        return Err(UpdateError::Network(format!("HTTP {}", response.status)));
    }
    let mut bytes = Vec::new();
    let mut buf = [0u8; 4096];
    let mut reader = response.body;
    loop {
        if cancel.is_some_and(TransferCancel::is_cancelled) {
            return Err(UpdateError::Cancelled);
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) if cancel.is_some_and(TransferCancel::is_cancelled) => {
                return Err(UpdateError::Cancelled);
            }
            Err(e) => return Err(UpdateError::Network(e.to_string())),
        };
        bytes.extend_from_slice(&buf[..n]);
        if bytes.len() as u64 > MAX_SUMS_BYTES {
            return Err(UpdateError::Network("checksum file too large".into()));
        }
    }
    String::from_utf8(bytes).map_err(|_| UpdateError::Malformed("checksums are not UTF-8".into()))
}

#[cfg(test)]
mod exchange_tests;
#[cfg(test)]
mod tests;
