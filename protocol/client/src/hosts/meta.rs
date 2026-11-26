//! Persistent `host_meta.json` store for per-host notices, packed 4:4:4 modes, capability
//! caches (no-op when unchanged), aliases, pinned addresses, and one-time import tracking.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use nightjar_policy::hosts::{
    EndpointSlot, HostCapabilitiesSnapshot, HostNames, HostNoticesState, HostPackedEntry,
    HostPackedModesState, PinnedAddress,
};
use serde::{Deserialize, Serialize};

use super::io::{GuardedJsonFile, dir_lock};

const HOST_META_FILE: &str = "host_meta.json";

/// All persisted per-host metadata for one UUID.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostMetaEntry {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub notices: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "HostPackedEntry::is_empty")]
    pub packed_modes: HostPackedEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_address: Option<EndpointSlot>,
}

impl HostMetaEntry {
    fn is_empty(&self) -> bool {
        self.notices.is_empty()
            && self.packed_modes.is_empty()
            && self.capabilities.is_none()
            && self.alias.is_none()
            && self.pinned_address.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct HostMetaFile {
    hosts: BTreeMap<String, HostMetaEntry>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    imported_sources: BTreeSet<String>,
}

/// Synchronized `host_meta.json` store.
#[derive(Clone, Debug)]
pub struct HostMetaStore {
    file: GuardedJsonFile,
}

impl HostMetaStore {
    pub fn open(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            file: GuardedJsonFile::new(dir.join(HOST_META_FILE), dir_lock(dir), true),
        }
    }

    pub fn path(&self) -> &Path {
        self.file.path()
    }

    fn read_unlocked(&self) -> HostMetaFile {
        self.file.read_json_or_default()
    }

    fn write_unlocked(&self, file: &mut HostMetaFile) -> io::Result<()> {
        file.hosts.retain(|_, entry| !entry.is_empty());
        let text = serde_json::to_string_pretty(file).map_err(io::Error::other)?;
        self.file.write_bytes(text.as_bytes())
    }

    fn with_notices_mut<R>(
        &self,
        f: impl FnOnce(&mut HostNoticesState) -> (R, bool),
    ) -> io::Result<R> {
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let mut state = HostNoticesState {
            hosts: file
                .hosts
                .iter()
                .filter(|(_, e)| !e.notices.is_empty())
                .map(|(k, e)| (k.clone(), e.notices.clone()))
                .collect(),
        };
        let (ret, changed) = f(&mut state);
        if changed {
            for entry in file.hosts.values_mut() {
                entry.notices.clear();
            }
            for (uuid, notices) in state.hosts {
                file.hosts.entry(uuid).or_default().notices = notices;
            }
            self.write_unlocked(&mut file)?;
        }
        Ok(ret)
    }

    fn with_packed_mut<R>(
        &self,
        f: impl FnOnce(&mut HostPackedModesState) -> (R, bool),
    ) -> io::Result<R> {
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let mut state = HostPackedModesState {
            hosts: file
                .hosts
                .iter()
                .filter(|(_, e)| !e.packed_modes.is_empty())
                .map(|(k, e)| (k.clone(), e.packed_modes.clone()))
                .collect(),
        };
        let (ret, changed) = f(&mut state);
        if changed {
            for entry in file.hosts.values_mut() {
                entry.packed_modes = HostPackedEntry::default();
            }
            for (uuid, packed) in state.hosts {
                file.hosts.entry(uuid).or_default().packed_modes = packed;
            }
            self.write_unlocked(&mut file)?;
        }
        Ok(ret)
    }

    pub fn claim_notice(&self, uuid: &str, notice: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(true);
        }
        self.with_notices_mut(|s| {
            let claimed = s.claim(uuid, notice);
            (claimed, claimed)
        })
    }

    pub fn reset_notice(&self, uuid: &str, notice: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        self.with_notices_mut(|s| {
            let removed = s.reset(uuid, notice);
            (removed, removed)
        })
    }

    pub fn reset_notice_everywhere(&self, notice: &str) -> io::Result<bool> {
        self.with_notices_mut(|s| {
            let changed = s.reset_everywhere(notice);
            (changed, changed)
        })
    }

    pub fn forget_notices(&self, uuid: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        self.with_notices_mut(|s| {
            let changed = s.forget(uuid);
            (changed, changed)
        })
    }

    pub fn packed_cached(&self, uuid: &str) -> i32 {
        if uuid.is_empty() {
            return -1;
        }
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        let Some(entry) = file.hosts.get(uuid) else {
            return -1;
        };
        let modes =
            i32::from(entry.packed_modes.mask.unwrap_or(0)) & nightjar_policy::hosts::ALL_PACKED_MODES;
        if modes > 0 { modes } else { -1 }
    }

    pub fn packed_store(&self, uuid: &str, describe_modes: i32) -> io::Result<bool> {
        if uuid.is_empty() || describe_modes < 0 {
            return Ok(false);
        }
        self.with_packed_mut(|s| {
            let changed = s.store(uuid, describe_modes);
            (changed, changed)
        })
    }

    pub fn packed_mark_refused(&self, uuid: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        self.with_packed_mut(|s| {
            let changed = s.mark_refused(uuid);
            (changed, changed)
        })
    }

    pub fn packed_refused(&self, uuid: &str) -> bool {
        if uuid.is_empty() {
            return false;
        }
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        file.hosts
            .get(uuid)
            .is_some_and(|e| e.packed_modes.refused)
    }

    pub fn packed_renew(&self, uuid: &str, stamp: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        self.with_packed_mut(|s| {
            let changed = s.renew(uuid, stamp);
            (changed, changed)
        })
    }

    pub fn packed_clear_refusals(&self) -> io::Result<bool> {
        self.with_packed_mut(|s| {
            let changed = s.clear_refusals();
            (changed, changed)
        })
    }

    pub fn load_capabilities(&self, uuid: &str) -> Option<String> {
        if uuid.is_empty() {
            return None;
        }
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        let raw = file.hosts.get(uuid)?.capabilities.as_deref()?;
        HostCapabilitiesSnapshot::parse(raw).map(|snap| snap.canonical_json().to_owned())
    }

    /// Saves normalized capabilities JSON. Returns `Ok(false)` (and writes nothing to disk) when
    /// `json` is unknown/malformed or normalizes equal to the already-saved snapshot.
    pub fn save_capabilities(&self, uuid: &str, json: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        let Some(incoming) = HostCapabilitiesSnapshot::parse(json) else {
            return Ok(false);
        };
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        if let Some(existing_raw) = file
            .hosts
            .get(uuid)
            .and_then(|e| e.capabilities.as_deref())
            && HostCapabilitiesSnapshot::parse(existing_raw).as_ref() == Some(&incoming)
        {
            return Ok(false);
        }
        file.hosts.entry(uuid.to_owned()).or_default().capabilities =
            Some(incoming.canonical_json().to_owned());
        self.write_unlocked(&mut file)?;
        Ok(true)
    }

    pub fn forget_capabilities(&self, uuid: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let Some(entry) = file.hosts.get_mut(uuid) else {
            return Ok(false);
        };
        if entry.capabilities.take().is_none() {
            return Ok(false);
        }
        self.write_unlocked(&mut file)?;
        Ok(true)
    }

    pub fn get_alias(&self, uuid: &str) -> Option<String> {
        if uuid.is_empty() {
            return None;
        }
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        file.hosts
            .get(uuid)
            .and_then(|e| e.alias.as_deref())
            .filter(|a| !a.is_empty())
            .map(ToOwned::to_owned)
    }

    pub fn set_alias(&self, uuid: &str, alias: Option<&str>) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        let normalized = HostNames::normalize_alias(alias, None);
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let entry = file.hosts.entry(uuid.to_owned()).or_default();
        if entry.alias == normalized {
            return Ok(false);
        }
        entry.alias = normalized;
        self.write_unlocked(&mut file)?;
        Ok(true)
    }

    pub fn get_pinned(&self, uuid: &str) -> Option<EndpointSlot> {
        if uuid.is_empty() {
            return None;
        }
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        file.hosts
            .get(uuid)
            .and_then(|e| e.pinned_address.clone())
    }

    pub fn set_pinned(&self, uuid: &str, slot: Option<EndpointSlot>) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        let entry = file.hosts.entry(uuid.to_owned()).or_default();
        if entry.pinned_address == slot {
            return Ok(false);
        }
        entry.pinned_address = slot;
        self.write_unlocked(&mut file)?;
        Ok(true)
    }

    /// Drops all metadata for `uuid` (notices, packed modes, capabilities, alias, pinned address).
    pub fn forget(&self, uuid: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        if file.hosts.remove(uuid).is_none() {
            return Ok(false);
        }
        self.write_unlocked(&mut file)?;
        Ok(true)
    }

    pub fn needs_import(&self, source: &str) -> bool {
        let _guard = self.file.lock();
        let file = self.read_unlocked();
        !file.imported_sources.contains(source)
    }

    /// One-time per-namespace import of legacy `SharedPreferences` entries. Records `source` in
    /// `imported_sources` so forgotten/cleared entries never resurrect on future runs.
    pub fn import_legacy_source(
        &self,
        source: &str,
        entries: &BTreeMap<String, serde_json::Value>,
    ) -> io::Result<bool> {
        let _guard = self.file.lock();
        let mut file = self.read_unlocked();
        if !file.imported_sources.insert(source.to_owned()) {
            return Ok(false);
        }
        match source {
            "HostNotices" => {
                let mut state = HostNoticesState::default();
                for (uuid, val) in entries {
                    if let Some(csv) = val.as_str() {
                        state.import_csv(uuid, csv);
                    }
                }
                for (uuid, set) in state.hosts {
                    file.hosts
                        .entry(uuid)
                        .or_default()
                        .notices
                        .extend(set);
                }
            }
            "HostPacked444Modes" => {
                for (key, val) in entries {
                    if let Some(uuid) = key.strip_prefix("refused:") {
                        if val.as_bool() == Some(true) {
                            file.hosts
                                .entry(uuid.to_owned())
                                .or_default()
                                .packed_modes
                                .refused = true;
                        }
                    } else if let Some(uuid) = key.strip_prefix("under:") {
                        if let Some(stamp) = val.as_str() {
                            file.hosts
                                .entry(uuid.to_owned())
                                .or_default()
                                .packed_modes
                                .stamp = Some(stamp.to_owned());
                        }
                    } else if let Some(raw_mask) = val.as_i64()
                        && raw_mask > 0
                    {
                        let masked = (raw_mask as i32 & nightjar_policy::hosts::ALL_PACKED_MODES) as u8;
                        if masked > 0 {
                            file.hosts
                                .entry(key.clone())
                                .or_default()
                                .packed_modes
                                .mask = Some(masked);
                        }
                    }
                }
            }
            "HostCapabilities" => {
                for (uuid, val) in entries {
                    if let Some(raw) = val.as_str()
                        && let Some(snap) = HostCapabilitiesSnapshot::parse(raw)
                    {
                        let entry = file.hosts.entry(uuid.clone()).or_default();
                        if entry.capabilities.is_none() {
                            entry.capabilities = Some(snap.canonical_json().to_owned());
                        }
                    }
                }
            }
            "HostNames" => {
                for (uuid, val) in entries {
                    if let Some(alias) = val.as_str()
                        && let Some(norm) = HostNames::normalize_alias(Some(alias), None)
                    {
                        let entry = file.hosts.entry(uuid.clone()).or_default();
                        if entry.alias.is_none() {
                            entry.alias = Some(norm);
                        }
                    }
                }
            }
            "PinnedAddresses" => {
                for (uuid, val) in entries {
                    if let Some(raw) = val.as_str()
                        && let Some(slot) = PinnedAddress::parse(raw)
                    {
                        let entry = file.hosts.entry(uuid.clone()).or_default();
                        if entry.pinned_address.is_none() {
                            entry.pinned_address = Some(slot);
                        }
                    }
                }
            }
            _ => {}
        }
        self.write_unlocked(&mut file)?;
        Ok(true)
    }
}
