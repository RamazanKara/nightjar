//! A relay's saved assignments, `wake-relay.json` in the app's config directory: what lets a
//! relay started again while the PC sleeps still wake it, and the PCs that refused it, so it
//! does not enroll itself with them again ([`super::driver`]). The assignments hold secrets:
//! the file is written atomically and, on Unix, readable by its owner only (0600), and neither
//! its contents nor a parse error's quote of them is ever printed.
use crate::hosts::io::write_atomic;
use crate::http::native_api::wake_relay::RelayAssignment;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "wake-relay.json";

/// Every PC this relay enrolled with or was refused by.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    #[serde(default)]
    pub hosts: Vec<SavedHost>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedHost {
    /// The PC's unique id, as its saved record has it.
    pub host: String,
    pub name: String,
    /// The PC's last assignment; `None` once it refused this relay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment: Option<RelayAssignment>,
    /// Why the PC refused this relay, in the PC's words: it is not enrolled again without
    /// `nightjar wake-relay --enroll`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

impl fmt::Debug for Saved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Saved").field("hosts", &self.hosts).finish()
    }
}

/// Without the assignment: it holds the relay's secret.
impl fmt::Debug for SavedHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedHost")
            .field("host", &self.host)
            .field("name", &self.name)
            .field("assignment", &self.assignment.is_some())
            .field("refused", &self.refused)
            .finish()
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// The saved file in `dir`, or nothing saved when there is none. A file that cannot be read is
/// an error, never an empty start: that would enroll again with a PC that refused this relay.
pub fn load(dir: &Path) -> io::Result<Saved> {
    let bytes = match std::fs::read(path(dir)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Saved::default()),
        Err(error) => return Err(error),
    };
    // Only where it broke: serde's message may quote a value, and a value may be a secret.
    serde_json::from_slice(&bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} cannot be read (line {}, column {}); fix it, or delete it and enroll again",
                path(dir).display(),
                error.line(),
                error.column()
            ),
        )
    })
}

/// Replaces the saved file atomically, owner-only on Unix.
pub fn save(dir: &Path, saved: &Saved) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(saved).map_err(io::Error::other)?;
    write_atomic(&path(dir), &bytes, true)
}

/// Deletes the saved file; none is fine.
pub fn remove(dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(path(dir)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("nightjar-wake-relay-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn assignment() -> RelayAssignment {
        RelayAssignment {
            target_id: "PC-UNIQUE".into(),
            target_name: "Desktop".into(),
            epoch: 2,
            secret: "ab".repeat(32),
            macs: vec!["10:7B:44:00:00:01".into()],
            broadcasts: vec!["192.168.4.255".into()],
            address: Some("192.168.4.20".into()),
            http_port: 47989,
            devices: vec!["cd".repeat(32)],
            refresh_seconds: 300,
        }
    }

    fn saved() -> Saved {
        Saved {
            hosts: vec![
                SavedHost {
                    host: "PC-UNIQUE".into(),
                    name: "Desktop".into(),
                    assignment: Some(assignment()),
                    refused: None,
                },
                SavedHost {
                    host: "OTHER".into(),
                    name: "Laptop".into(),
                    assignment: None,
                    refused: Some("That wake relay is not enrolled on this PC.".into()),
                },
            ],
        }
    }

    #[test]
    fn what_is_saved_reads_back_and_nothing_saved_is_an_empty_start() {
        let dir = temp_dir("roundtrip");
        assert_eq!(load(&dir).unwrap(), Saved::default());
        save(&dir, &saved()).unwrap();
        assert_eq!(load(&dir).unwrap(), saved());
        remove(&dir).unwrap();
        assert_eq!(load(&dir).unwrap(), Saved::default());
        remove(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_file_is_the_owners_only() {
        let dir = temp_dir("private");
        save(&dir, &saved()).unwrap();
        // OS intent: only Unix has the owner-only mode this checks; Windows keeps the file in
        // the user's own profile.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path(&dir)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_file_is_an_error_that_quotes_none_of_it() {
        let dir = temp_dir("broken");
        let secret = "ef".repeat(32);
        // serde would say `invalid type: string "efef…", expected u64`.
        let text = format!(
            r#"{{"hosts":[{{"host":"PC","name":"D","assignment":{{"target_id":"PC","epoch":"{secret}"}}}}]}}"#
        );
        std::fs::write(path(&dir), text).unwrap();
        let error = load(&dir).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!error.to_string().contains(&secret), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn debug_output_leaves_the_secret_out() {
        let text = format!("{:?}", saved());
        assert!(!text.contains(&"ab".repeat(32)), "{text}");
        assert!(text.contains("PC-UNIQUE"));
    }
}
