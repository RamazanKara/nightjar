//! GitHub release parsing, Android/Desktop asset selection and SHA256SUMS parsing.

use serde::{Deserialize, Serialize};

use super::semver::SemVer;

const DESKTOP_REPO: &str = "RamazanKara/nightjar";
/// Desktop releases are around 12 MB.
const DESKTOP_MAX_SIZE: u64 = 512 << 20;

/// Returns `true` if `name` is a single relative filename without path separators, drive prefixes,
/// NUL bytes, or `.` / `..` traversal segments.
pub fn is_safe_asset_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && !name.contains("..")
        && !name.contains(['/', '\\', '\0', ':'])
}

/// One downloadable asset attached to a GitHub release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// Verified Desktop installer asset metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopAsset {
    pub name: String,
    pub url: String,
    pub digest: String,
    pub size: u64,
}

/// Why a Desktop release has no runnable asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopAssetError {
    NoInstaller,
    NoDigest,
    Address(String),
    Size,
}

/// The parts of a GitHub release that the Android and Desktop updaters inspect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub body: String,
    #[serde(default)]
    pub html_url: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
}

impl Release {
    /// Parses GitHub's `/releases/latest` JSON response.
    pub fn parse(json: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("unreadable release: {e}"))?;
        let tag = value
            .get("tag_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "the release has no tag".to_owned())?
            .to_owned();
        let body = match value.get("body") {
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => String::new(),
        };
        let html_url = value
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let draft = value
            .get("draft")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let prerelease = value
            .get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut assets = Vec::new();
        if let Some(list) = value.get("assets").and_then(|v| v.as_array()) {
            for item in list {
                let Some(obj) = item.as_object() else {
                    continue;
                };
                let Some(name) = obj.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(url) = obj.get("browser_download_url").and_then(|v| v.as_str()) else {
                    continue;
                };
                let size = obj.get("size").and_then(|v| v.as_i64()).unwrap_or(-1);
                let digest = obj
                    .get("digest")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                assets.push(ReleaseAsset {
                    name: name.to_owned(),
                    url: url.to_owned(),
                    size,
                    digest,
                });
            }
        }
        Ok(Self {
            tag,
            body,
            html_url,
            draft,
            prerelease,
            assets,
        })
    }

    /// The version without a leading `v`.
    pub fn version(&self) -> String {
        SemVer::normalize(&self.tag)
    }

    /// Whether this release is a final one newer than `current_version`.
    pub fn is_update_for(&self, current_version: &str) -> bool {
        !self.draft
            && !self.prerelease
            && SemVer::parse(&self.tag).is_some_and(|r| !r.is_prerelease())
            && SemVer::is_newer(&self.tag, current_version)
    }

    /// The APK for this build: `nightjar-X.Y.Z.apk`, or `nightjar-X.Y.Z-root.apk` for root.
    pub fn apk_asset(&self, root: bool) -> Option<&ReleaseAsset> {
        let mut fallback = None;
        for asset in &self.assets {
            let lower = asset.name.to_ascii_lowercase();
            if !lower.ends_with(".apk") || lower.ends_with("-root.apk") != root {
                continue;
            }
            if lower.starts_with("nightjar-") {
                return Some(asset);
            }
            if fallback.is_none() {
                fallback = Some(asset);
            }
        }
        fallback
    }

    /// `SHA256SUMS-X.Y.Z.txt`, or `None` when the release has none.
    pub fn checksum_asset(&self) -> Option<&ReleaseAsset> {
        let wanted = format!("sha256sums-{}.txt", self.version().to_ascii_lowercase());
        let mut fallback = None;
        for asset in &self.assets {
            let lower = asset.name.to_ascii_lowercase();
            if !lower.starts_with("sha256sums") || !lower.ends_with(".txt") {
                continue;
            }
            if lower == wanted {
                return Some(asset);
            }
            if fallback.is_none() {
                fallback = Some(asset);
            }
        }
        fallback
    }

    /// Whether `asset.url` matches the pinned GitHub release download address for this release tag.
    pub fn is_pinned_download_url(&self, asset: &ReleaseAsset) -> bool {
        let expected_url = format!(
            "https://github.com/RamazanKara/nightjar/releases/download/{}/{}",
            self.tag, asset.name
        );
        asset.url == expected_url
    }

    /// Validates and returns the named Desktop release asset.
    pub fn desktop_asset(&self, name: &str) -> Result<DesktopAsset, DesktopAssetError> {
        if !is_safe_asset_name(name) {
            return Err(DesktopAssetError::NoInstaller);
        }
        let found = self
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .ok_or(DesktopAssetError::NoInstaller)?;
        let expected_url = format!(
            "https://github.com/{DESKTOP_REPO}/releases/download/{}/{name}",
            self.tag
        );
        if found.url != expected_url {
            return Err(DesktopAssetError::Address(found.url.clone()));
        }
        let digest = found
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(DesktopAssetError::NoDigest)?;
        let size = u64::try_from(found.size)
            .ok()
            .filter(|s| *s > 0 && *s <= DESKTOP_MAX_SIZE)
            .ok_or(DesktopAssetError::Size)?;
        Ok(DesktopAsset {
            name: name.to_owned(),
            url: expected_url,
            digest: digest.to_ascii_lowercase(),
            size,
        })
    }

    /// Picks the first preferred Desktop asset name present in this release.
    pub fn select_desktop_asset(
        &self,
        names: &[String],
    ) -> Result<DesktopAsset, DesktopAssetError> {
        names
            .iter()
            .map(|name| self.desktop_asset(name))
            .find(|res| !matches!(res, Err(DesktopAssetError::NoInstaller)))
            .unwrap_or(Err(DesktopAssetError::NoInstaller))
    }
}

/// Reads `sha256sum` output (`hex  name` or `hex *name`) and formats digests.
pub struct ChecksumFile;

impl ChecksumFile {
    /// Returns the lowercase hex digest listed for `file_name`, or `None` when absent.
    pub fn find(sums: &str, file_name: &str) -> Option<String> {
        if file_name.is_empty() {
            return None;
        }
        for raw_line in sums.lines() {
            let line = raw_line.trim();
            if line.len() < 66 || !line.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit) {
                continue;
            }
            let (hex_part, rest) = line.split_at(64);
            let sep_len = rest
                .bytes()
                .take_while(|&b| b == b' ' || b == b'\t')
                .count();
            if sep_len == 0 {
                continue;
            }
            let mut target = rest[sep_len..].trim_end();
            if let Some(after_star) = target.strip_prefix('*') {
                target = after_star;
            }
            if target.is_empty() {
                continue;
            }
            let base = target
                .rsplit_once(['/', '\\'])
                .map_or(target, |(_, name)| name);
            if base == file_name {
                return Some(hex_part.to_ascii_lowercase());
            }
        }
        None
    }

    pub fn hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "{b:02x}");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn android_asset(name: &str, size: i64) -> String {
        format!(
            r#"{{"name":"{name}","size":{size},"browser_download_url":"https://github.com/RamazanKara/nightjar/releases/download/v0.7.0/{name}"}}"#
        )
    }

    #[test]
    fn parses_release_and_picks_apk_and_checksum_assets() {
        let json = format!(
            r###"{{"tag_name":"v0.7.0","body":"## Changes\n- Faster","draft":false,"prerelease":false,"assets":[{},{},{},{}]}}"###,
            android_asset("nightjar-0.7.0-root.apk", 18008391),
            android_asset("nightjar-0.7.0.aab", 25129586),
            android_asset("nightjar-0.7.0.apk", 17987490),
            android_asset("SHA256SUMS-0.7.0.txt", 260)
        );
        let release = Release::parse(&json).unwrap();
        assert_eq!(release.version(), "0.7.0");
        assert_eq!(release.apk_asset(false).unwrap().name, "nightjar-0.7.0.apk");
        assert_eq!(
            release.apk_asset(true).unwrap().name,
            "nightjar-0.7.0-root.apk"
        );
        assert_eq!(
            release.checksum_asset().unwrap().name,
            "SHA256SUMS-0.7.0.txt"
        );
        assert!(release.is_update_for("0.6.0"));
        assert!(release.is_update_for("0.6.9-debug"));
        assert!(!release.is_update_for("0.7.0"));
    }

    #[test]
    fn desktop_asset_validation_checks_url_digest_and_size() {
        let name = "nightjar-desktop-4.1.0-windows-x86_64-setup.exe";
        let good_url =
            format!("https://github.com/RamazanKara/nightjar/releases/download/v4.1.0/{name}");
        let digest = "sha256:AB12000000000000000000000000000000000000000000000000000000000000";
        let json = format!(
            r#"{{"tag_name":"v4.1.0","draft":false,"prerelease":false,"html_url":"https://github.com/RamazanKara/nightjar/releases/tag/v4.1.0","assets":[{{"name":"{name}","size":1000,"browser_download_url":"{good_url}","digest":"{digest}"}}]}}"#
        );
        let release = Release::parse(&json).unwrap();
        let asset = release.desktop_asset(name).unwrap();
        assert_eq!(asset.digest, digest[7..].to_ascii_lowercase());
        assert_eq!(asset.size, 1000);

        let evil_json = format!(
            r#"{{"tag_name":"v4.1.0","assets":[{{"name":"{name}","size":1000,"browser_download_url":"https://evil.example/{name}","digest":"{digest}"}}]}}"#
        );
        let evil = Release::parse(&evil_json).unwrap();
        assert_eq!(
            evil.desktop_asset(name),
            Err(DesktopAssetError::Address(format!(
                "https://evil.example/{name}"
            )))
        );

        let no_digest = format!(
            r#"{{"tag_name":"v4.1.0","assets":[{{"name":"{name}","size":1000,"browser_download_url":"{good_url}","digest":"md5:abcd"}}]}}"#
        );
        assert_eq!(
            Release::parse(&no_digest).unwrap().desktop_asset(name),
            Err(DesktopAssetError::NoDigest)
        );
    }

    #[test]
    fn checksum_file_matches_binary_text_and_path_entries() {
        let apk = "454f9e4a7f46253dcfa138208ea392a8e3c43e862659ef559b060f3d8fb1fd80";
        let sums = format!("{apk} *nightjar-0.7.0.apk\r\n");
        assert_eq!(
            ChecksumFile::find(&sums, "nightjar-0.7.0.apk").as_deref(),
            Some(apk)
        );
        assert_eq!(
            ChecksumFile::find(
                &format!(
                    "{}  app/build/outputs/nightjar-0.7.0.apk\n",
                    apk.to_uppercase()
                ),
                "nightjar-0.7.0.apk"
            )
            .as_deref(),
            Some(apk)
        );
        assert_eq!(
            ChecksumFile::find("# comment\n", "nightjar-0.7.0.apk"),
            None
        );
        // Non-ASCII line >= 66 bytes where byte 64 splits a multi-byte UTF-8 char must not panic.
        let multibyte_at_64 = format!(
            "{}é  ignored.apk\n{}  nightjar-0.7.0.apk\n",
            "a".repeat(63),
            apk
        );
        assert_eq!(
            ChecksumFile::find(&multibyte_at_64, "nightjar-0.7.0.apk").as_deref(),
            Some(apk)
        );
        let comment_at_64 = format!("# {}  ignored\n", "é".repeat(40));
        assert_eq!(ChecksumFile::find(&comment_at_64, "nightjar-0.7.0.apk"), None);
        assert_eq!(ChecksumFile::hex(&[0, 0xff, 0x10]), "00ff10");
    }

    #[test]
    fn is_safe_asset_name_rejects_traversal_separators_and_special_chars() {
        assert!(is_safe_asset_name("nightjar-0.7.0.apk"));
        assert!(is_safe_asset_name(
            "nightjar-desktop-4.1.0-windows-x86_64-setup.exe"
        ));
        for bad in [
            "",
            ".",
            "..",
            "../evil.apk",
            "..\\evil.exe",
            "a/b.apk",
            "a\\b.apk",
            "C:evil.exe",
            "evil\0.apk",
            "nightjar..apk",
        ] {
            assert!(!is_safe_asset_name(bad), "must reject {bad:?}");
        }

        let unsafe_name = "../evil-setup.exe";
        let rel = Release {
            tag: "v4.1.0".into(),
            body: String::new(),
            html_url: String::new(),
            draft: false,
            prerelease: false,
            assets: vec![ReleaseAsset {
                name: unsafe_name.into(),
                url: format!(
                    "https://github.com/RamazanKara/nightjar/releases/download/v4.1.0/{unsafe_name}"
                ),
                size: 1000,
                digest: Some(
                    "sha256:ab12000000000000000000000000000000000000000000000000000000000000"
                        .into(),
                ),
            }],
        };
        assert_eq!(
            rel.desktop_asset(unsafe_name),
            Err(DesktopAssetError::NoInstaller)
        );
    }
}
