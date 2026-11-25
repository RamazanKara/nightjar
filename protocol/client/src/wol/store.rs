//! Atomic, `0600`-protected `wake_targets.json` persistence with one-time legacy import.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use nightjar_policy::wol::{MAX_BROADCASTS, MAX_MACS, extract_targets, normal_mac, parse_ipv4_literal};
use rubylight_native_api::wake::Wake;
use serde::{Deserialize, Serialize};

use crate::hosts::io::{GuardedJsonFile, dir_lock};

const WAKE_TARGETS_FILE: &str = "wake_targets.json";

/// Stored Wake-on-LAN targets for one host.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WakeTargetEntry {
    pub macs: Vec<String>,
    pub broadcasts: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct WakeTargetsFile {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    imported: bool,
    hosts: BTreeMap<String, WakeTargetEntry>,
    manual: BTreeMap<String, Vec<String>>,
}

/// Persistent `wake_targets.json` store synchronized per canonical directory.
#[derive(Clone, Debug)]
pub struct WakeTargetsStore {
    file: GuardedJsonFile,
}

impl WakeTargetsStore {
    pub fn open(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            file: GuardedJsonFile::new(dir.join(WAKE_TARGETS_FILE), dir_lock(dir), true),
        }
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    fn read_unlocked(&self) -> WakeTargetsFile {
        self.file.read_json_or_default()
    }

    fn write_unlocked(&self, file: &WakeTargetsFile) -> io::Result<()> {
        let text = serde_json::to_string_pretty(file).map_err(io::Error::other)?;
        self.file.write_bytes(text.as_bytes())
    }

    /// Loads merged targets (`hosts[uuid]` + deduplicated `manual[uuid]`) for `uuid`.
    pub fn load(&self, uuid: &str) -> WakeTargetEntry {
        if uuid.is_empty() {
            return WakeTargetEntry::default();
        }
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        let mut macs = Vec::new();
        let mut broadcasts = Vec::new();
        if let Some(entry) = file.hosts.get(uuid) {
            for mac in entry.macs.iter().filter_map(|m| normal_mac(m)) {
                if macs.len() < MAX_MACS && !macs.contains(&mac) {
                    macs.push(mac);
                }
            }
            for b in &entry.broadcasts {
                if let Some(octets) = parse_ipv4_literal(b) {
                    let norm = std::net::Ipv4Addr::from(octets).to_string();
                    if broadcasts.len() < MAX_BROADCASTS && !broadcasts.contains(&norm) {
                        broadcasts.push(norm);
                    }
                }
            }
        }
        if let Some(manual) = file.manual.get(uuid) {
            for mac in manual.iter().filter_map(|m| normal_mac(m)) {
                if !macs.contains(&mac) {
                    macs.push(mac);
                }
            }
        }
        WakeTargetEntry { macs, broadcasts }
    }

    /// Returns normalized user-added MACs for `uuid`.
    pub fn manual(&self, uuid: &str) -> Vec<String> {
        if uuid.is_empty() {
            return Vec::new();
        }
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        let mut macs = Vec::new();
        if let Some(list) = file.manual.get(uuid) {
            for mac in list.iter().filter_map(|m| normal_mac(m)) {
                if macs.len() < MAX_MACS && !macs.contains(&mac) {
                    macs.push(mac);
                }
            }
        }
        macs
    }

    pub fn store_manual(&self, uuid: &str, macs: &[String]) -> io::Result<()> {
        if uuid.is_empty() {
            return Ok(());
        }
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let mut normalized = Vec::new();
        for mac in macs.iter().filter_map(|m| normal_mac(m)) {
            if normalized.len() < MAX_MACS && !normalized.contains(&mac) {
                normalized.push(mac);
            }
        }
        if normalized.is_empty() {
            if file.manual.remove(uuid).is_none() {
                return Ok(());
            }
        } else if file.manual.get(uuid) == Some(&normalized) {
            return Ok(());
        } else {
            file.manual.insert(uuid.to_owned(), normalized);
        }
        self.write_unlocked(&file)
    }

    pub fn store_wake(&self, uuid: &str, wake: &Wake) -> io::Result<()> {
        let (macs, broadcasts) = nightjar_policy::wol::targets(wake);
        self.store_normalized(uuid, macs, broadcasts)
    }

    /// Normalizes and stores raw `(primary, [(mac, broadcasts)])` targets for `uuid`.
    pub fn store_raw<'a>(
        &self,
        uuid: &str,
        primary: Option<&'a str>,
        targets: impl IntoIterator<Item = (&'a str, &'a [String])>,
    ) -> io::Result<()> {
        let (macs, broadcasts) = extract_targets(primary, targets);
        self.store_normalized(uuid, macs, broadcasts)
    }

    fn store_normalized(
        &self,
        uuid: &str,
        macs: Vec<String>,
        broadcasts: Vec<String>,
    ) -> io::Result<()> {
        if uuid.is_empty() {
            return Ok(());
        }
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let entry = WakeTargetEntry { macs, broadcasts };
        if file.hosts.get(uuid) == Some(&entry) {
            return Ok(());
        }
        file.hosts.insert(uuid.to_owned(), entry);
        self.write_unlocked(&file)
    }

    /// Drops host-reported targets while keeping user-added `manual` MACs.
    pub fn forget(&self, uuid: &str) -> io::Result<()> {
        if uuid.is_empty() {
            return Ok(());
        }
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        if file.hosts.remove(uuid).is_none() {
            return Ok(());
        }
        self.write_unlocked(&file)
    }

    /// Drops both host-reported targets and user-added `manual` MACs for a removed PC.
    pub fn forget_host(&self, uuid: &str) -> io::Result<()> {
        if uuid.is_empty() {
            return Ok(());
        }
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let a = file.hosts.remove(uuid).is_some();
        let b = file.manual.remove(uuid).is_some();
        if !a && !b {
            return Ok(());
        }
        self.write_unlocked(&file)
    }

    pub fn has_mac(&self, uuid: &str, primary_mac: Option<&str>) -> bool {
        primary_mac.is_some() || !self.load(uuid).macs.is_empty()
    }

    /// Idempotent one-time import from legacy `SharedPreferences("wake_targets")`: preserves any
    /// entries already present in `wake_targets.json`.
    pub fn import_legacy(
        &self,
        hosts: BTreeMap<String, WakeTargetEntry>,
        manual: BTreeMap<String, Vec<String>>,
    ) -> io::Result<bool> {
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        if file.imported {
            return Ok(false);
        }
        let had_payload = !hosts.is_empty() || !manual.is_empty();
        let mut changed = false;
        for (uuid, entry) in hosts {
            if uuid.is_empty() || file.hosts.contains_key(&uuid) {
                continue;
            }
            let macs: Vec<String> = entry
                .macs
                .iter()
                .filter_map(|m| normal_mac(m))
                .take(MAX_MACS)
                .collect();
            let broadcasts: Vec<String> = entry
                .broadcasts
                .iter()
                .filter_map(|b| parse_ipv4_literal(b).map(|o| std::net::Ipv4Addr::from(o).to_string()))
                .take(MAX_BROADCASTS)
                .collect();
            file.hosts
                .insert(uuid, WakeTargetEntry { macs, broadcasts });
            changed = true;
        }
        for (uuid, list) in manual {
            if uuid.is_empty() || file.manual.contains_key(&uuid) {
                continue;
            }
            let mut normalized = Vec::new();
            for mac in list.iter().filter_map(|m| normal_mac(m)) {
                if normalized.len() < MAX_MACS && !normalized.contains(&mac) {
                    normalized.push(mac);
                }
            }
            if !normalized.is_empty() {
                file.manual.insert(uuid, normalized);
                changed = true;
            }
        }
        if changed || (!file.imported && (had_payload || self.file.path().exists())) {
            file.imported = true;
            self.write_unlocked(&file)?;
        }
        Ok(changed)
    }
}
