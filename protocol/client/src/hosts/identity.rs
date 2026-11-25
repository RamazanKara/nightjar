//! Persistent `identity.json` store (`IdentityStore`) with `0o600` private atomic writes,
//! `.bad` damaged-file backup + overwrite refusal, and one-time import of legacy Android
//! `uniqueid` + `client.crt` + `client.key`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nightjar_policy::hosts::IdentityFile;

use super::io::{UnreadableGuard, back_up, dir_lock, write_atomic};
use crate::http::{ClientIdentity, crypto, der};

const IDENTITY_FILE: &str = "identity.json";

/// In-memory and serialized representation of a loaded or generated client identity.
#[derive(Clone, Debug)]
pub struct LoadedIdentity {
    pub unique_id: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
    pub certificate_der: Vec<u8>,
    pub private_key_pkcs8: Vec<u8>,
    pub persisted: bool,
}

impl LoadedIdentity {
    fn from_client(client: &ClientIdentity, persisted: bool) -> Self {
        Self {
            unique_id: client.unique_id().to_owned(),
            certificate_pem: client.certificate_pem().to_owned(),
            private_key_pem: client.private_key_pem().to_owned(),
            certificate_der: client.certificate_der().to_vec(),
            private_key_pkcs8: client.private_key_pkcs8().to_vec(),
            persisted,
        }
    }
}

/// Synchronized `identity.json` store.
#[derive(Clone, Debug)]
pub struct IdentityStore {
    dir: PathBuf,
    read_only: bool,
    lock: Arc<std::sync::Mutex<()>>,
    unreadable: Arc<UnreadableGuard>,
}

impl IdentityStore {
    pub fn open(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        Self {
            lock: dir_lock(&dir),
            dir,
            read_only: false,
            unreadable: Arc::new(UnreadableGuard::default()),
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self) -> PathBuf {
        self.dir.join(IDENTITY_FILE)
    }

    fn read_unlocked(&self) -> Result<Option<IdentityFile>, String> {
        let path = self.path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.unreadable.clear(&path);
                return Ok(None);
            }
            Err(e) => {
                self.unreadable.leave_as_it_is(&path);
                return Err(format!("{} cannot be read: {e}", path.display()));
            }
        };
        match serde_json::from_str::<IdentityFile>(&text) {
            Ok(identity) => {
                self.unreadable.clear(&path);
                Ok(Some(identity))
            }
            Err(e) => {
                if !self.read_only
                    && let Err(copy) = back_up(&path, "bad")
                {
                    eprintln!("nightjar: could not keep a copy of {}: {copy}", path.display());
                }
                self.unreadable.leave_as_it_is(&path);
                Err(format!("{} cannot be read: {e}", path.display()))
            }
        }
    }

    /// Reads `identity.json`: returns `Ok(None)` only when absent. Any unreadable or damaged file
    /// is backed up to `identity.json.bad` and protected against replacement.
    pub fn identity(&self) -> Result<Option<IdentityFile>, String> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.read_unlocked()
    }

    /// Reads and validates an existing `identity.json` into a [`ClientIdentity`] without
    /// generating or persisting a new identity when absent.
    pub fn load_existing_client(&self) -> Result<Option<ClientIdentity>, String> {
        let Some(saved) = self.identity()? else {
            return Ok(None);
        };
        ClientIdentity::from_pem(saved.unique_id, &saved.certificate_pem, &saved.private_key_pem)
            .map(Some)
            .map_err(|e| e.to_string())
    }

    /// Saves `identity.json` (`0o600`), refusing to overwrite a file that could not be read.
    pub fn save_identity(&self, identity: &IdentityFile) -> io::Result<()> {
        if self.read_only {
            return Ok(());
        }
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.path();
        self.unreadable.check_writable(&path)?;
        if path.exists() && self.read_unlocked().is_err() {
            return Err(io::Error::other(format!(
                "{} could not be read, so it is left as it is to keep what it holds",
                path.display()
            )));
        }
        let text = serde_json::to_string_pretty(identity).map_err(io::Error::other)?;
        write_atomic(&path, text.as_bytes(), true)
    }

    /// Loads `identity.json` if present (refusing to overwrite a corrupt file), otherwise imports
    /// valid legacy Android credentials (`uniqueid` + `client.crt` + `client.key`) or generates a
    /// fresh 2048-bit RSA identity + 16-hex-char `unique_id` and persists it as `identity.json`.
    pub fn load_or_create(
        &self,
        legacy_unique_id: Option<&str>,
        legacy_cert_pem: Option<&str>,
        legacy_key_pkcs8: Option<&[u8]>,
    ) -> Result<LoadedIdentity, String> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(saved) = self.read_unlocked()? {
            let client = ClientIdentity::from_pem(
                saved.unique_id,
                &saved.certificate_pem,
                &saved.private_key_pem,
            )
            .map_err(|e| e.to_string())?;
            return Ok(LoadedIdentity::from_client(&client, true));
        }

        let uid = legacy_unique_id
            .filter(|id| is_valid_legacy_uid(id))
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(random_unique_id);

        let client = if let (Some(cert_pem), Some(key_pkcs8)) = (legacy_cert_pem, legacy_key_pkcs8)
            && let Ok(cert_der) = der::pem_decode(cert_pem, "CERTIFICATE")
            && let Ok(imported) =
                ClientIdentity::from_der(uid.clone(), cert_der, key_pkcs8.to_vec())
        {
            imported
        } else {
            let generated = ClientIdentity::generate().map_err(|e| e.to_string())?;
            if legacy_unique_id.is_some_and(is_valid_legacy_uid) {
                ClientIdentity::from_der(
                    uid,
                    generated.certificate_der().to_vec(),
                    generated.private_key_pkcs8().to_vec(),
                )
                .map_err(|e| e.to_string())?
            } else {
                generated
            }
        };

        let file = IdentityFile {
            unique_id: client.unique_id().to_owned(),
            certificate_pem: client.certificate_pem().to_owned(),
            private_key_pem: client.private_key_pem().to_owned(),
        };
        let persisted = if self.read_only {
            false
        } else {
            let path = self.path();
            match serde_json::to_string_pretty(&file)
                .map_err(io::Error::other)
                .and_then(|text| write_atomic(&path, text.as_bytes(), true))
            {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("nightjar: saving identity failed ({e}); keeping in-memory identity");
                    false
                }
            }
        };
        Ok(LoadedIdentity::from_client(&client, persisted))
    }
}

fn is_valid_legacy_uid(uid: &str) -> bool {
    uid.len() == 16 && uid.bytes().all(|b| b.is_ascii_hexdigit())
}

fn random_unique_id() -> String {
    der::hex_encode(&crypto::random::<8>()).to_ascii_lowercase()
}
