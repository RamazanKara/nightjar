//! Persistent `hosts.json` registry (`HostStore`), sharing a lossless JSON array format
//! between Desktop `HostRecord` and Android `HostEntry` with DER<->PEM conversion,
//! change-detecting upserts, `.bad` backup, unreadable refusal, and one-time legacy import.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nightjar_policy::hosts::{
    EndpointSlot, HostSlots, SharedHostFields, keep_readable_hosts, sync_shared_from_slots,
    sync_slots_from_shared,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::io::{UnreadableGuard, back_up, dir_lock, write_atomic};
use crate::http::der::{pem_decode, pem_encode};

const HOSTS_FILE: &str = "hosts.json";
const MIGRATED_MARKER: &str = ".hosts_migrated";

/// Unified persistent host entry in `hosts.json` combining Desktop fields, Android's distinct
/// typed `(address, port)` slots, and unknown-field pass-through via `extra`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostEntry {
    #[serde(flatten)]
    pub shared: SharedHostFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_endpoint: Option<EndpointSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_endpoint: Option<EndpointSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_endpoint: Option<EndpointSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ipv6_endpoint: Option<EndpointSlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_endpoint: Option<EndpointSlot>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl HostEntry {
    /// Decodes the pinned server certificate PEM into X.509 DER bytes, if present and valid.
    pub fn server_certificate_der(&self) -> Option<Vec<u8>> {
        let pem = self.shared.server_certificate.as_deref()?;
        pem_decode(pem, "CERTIFICATE").ok()
    }

    /// Encodes X.509 DER bytes as PEM into `shared.server_certificate` (or clears when `None`).
    pub fn set_server_certificate_der(&mut self, der: Option<&[u8]>) {
        self.shared.server_certificate = der.map(|bytes| pem_encode(bytes, "CERTIFICATE"));
    }

    /// Typed Android/shared endpoint slots for this entry.
    pub fn slots(&self) -> HostSlots {
        HostSlots {
            local: self.local_endpoint.clone(),
            manual: self.manual_endpoint.clone(),
            ipv6: self.ipv6_endpoint.clone(),
            remote: self.remote_endpoint.clone(),
            pinned: self.pinned_endpoint.clone(),
        }
    }

    /// Writes `slots` back into this entry's typed endpoint fields.
    pub fn set_slots(&mut self, slots: &HostSlots) {
        self.local_endpoint = slots.local.clone();
        self.manual_endpoint = slots.manual.clone();
        self.ipv6_endpoint = slots.ipv6.clone();
        self.remote_endpoint = slots.remote.clone();
        self.pinned_endpoint = slots.pinned.clone();
    }

    /// Whether any of the four candidate Android endpoint slots is populated.
    pub fn has_android_slots(&self) -> bool {
        self.local_endpoint.is_some()
            || self.manual_endpoint.is_some()
            || self.ipv6_endpoint.is_some()
            || self.remote_endpoint.is_some()
    }

    /// Recomputes `shared.address`, `shared.http_port`, `shared.remote_address`, `shared.manual`,
    /// `shared.other_addresses`, `shared.other_http_ports`, and `shared.pinned_address` from slots.
    pub fn sync_shared_from_slots(&mut self) {
        let slots = self.slots();
        sync_shared_from_slots(&mut self.shared, &slots);
    }

    /// Populates missing typed slots from legacy flat `shared.*` fields.
    pub fn sync_slots_from_shared(&mut self) {
        let mut slots = self.slots();
        sync_slots_from_shared(&self.shared, &mut slots);
        self.set_slots(&slots);
    }
}

/// Synchronized `hosts.json` store for both Desktop (`HostRecord`) and Android (`HostEntry`).
#[derive(Clone, Debug)]
pub struct HostStore {
    dir: PathBuf,
    read_only: bool,
    lock: Arc<std::sync::Mutex<()>>,
    unreadable: Arc<UnreadableGuard>,
}

fn merge_preserving_desktop(existing: &HostEntry, mut incoming: HostEntry) -> HostEntry {
    for (key, value) in &existing.extra {
        incoming
            .extra
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
    if incoming.pinned_endpoint.is_none() {
        incoming.pinned_endpoint = existing.pinned_endpoint.clone();
    }
    if incoming.shared.alias.is_none() {
        incoming.shared.alias = existing.shared.alias.clone();
    }
    if incoming.shared.https_port == 0 {
        incoming.shared.https_port = existing.shared.https_port;
    }
    let mut merged_addrs = Vec::new();
    for addr in existing
        .shared
        .other_addresses
        .iter()
        .chain(incoming.shared.other_addresses.iter())
    {
        if !addr.is_empty()
            && *addr != incoming.shared.address
            && incoming.shared.remote_address.as_deref() != Some(addr.as_str())
            && !merged_addrs.contains(addr)
        {
            merged_addrs.push(addr.clone());
        }
    }
    incoming.shared.other_addresses = merged_addrs;

    let mut merged_ports = Vec::new();
    for &port in existing
        .shared
        .other_http_ports
        .iter()
        .chain(incoming.shared.other_http_ports.iter())
    {
        if port != 0 && port != incoming.shared.http_port && !merged_ports.contains(&port) {
            merged_ports.push(port);
        }
    }
    incoming.shared.other_http_ports = merged_ports;

    if incoming.shared.pinned_address.is_none() {
        incoming.shared.pinned_address = existing.shared.pinned_address.clone();
    }
    if incoming.shared.wake_macs.is_empty() {
        incoming.shared.wake_macs = existing.shared.wake_macs.clone();
    }
    if incoming.shared.wake_broadcasts.is_empty() {
        incoming.shared.wake_broadcasts = existing.shared.wake_broadcasts.clone();
    }
    if incoming.shared.own_wake_macs.is_empty() {
        incoming.shared.own_wake_macs = existing.shared.own_wake_macs.clone();
    }
    if incoming.shared.quit_on_leave.is_empty() {
        incoming.shared.quit_on_leave = existing.shared.quit_on_leave.clone();
    }
    if incoming.shared.notices.is_empty() {
        incoming.shared.notices = existing.shared.notices.clone();
    }
    if incoming.shared.full_chroma == Default::default() {
        incoming.shared.full_chroma = existing.shared.full_chroma.clone();
    }
    if incoming.shared.internet_play.is_none() {
        incoming.shared.internet_play = existing.shared.internet_play.clone();
    }
    if existing.shared.manual
        && incoming.manual_endpoint.is_some()
        && incoming.local_endpoint.is_none()
    {
        incoming.shared.manual = true;
    }
    incoming
}

impl HostStore {
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
        self.dir.join(HOSTS_FILE)
    }

    fn marker_path(&self) -> PathBuf {
        self.dir.join(MIGRATED_MARKER)
    }

    fn read_kept_unlocked<T, F>(&self, mut keep_settings: F) -> Vec<T>
    where
        T: DeserializeOwned,
        F: FnMut(&mut Value),
    {
        let path = self.path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.unreadable.clear(&path);
                return Vec::new();
            }
            Err(e) => {
                eprintln!(
                    "nightjar: cannot read {} ({e}); it is left as it is",
                    path.display()
                );
                self.unreadable.leave_as_it_is(&path);
                return Vec::new();
            }
        };
        if let Ok(list) = serde_json::from_str::<Vec<T>>(&text) {
            self.unreadable.clear(&path);
            return list;
        }
        eprintln!("nightjar: {} can't be read whole", path.display());
        let backup_failed = if !self.read_only
            && let Err(e) = back_up(&path, "bad")
        {
            eprintln!("nightjar: could not keep a copy of {}: {e}", path.display());
            true
        } else {
            false
        };
        let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
            if self.read_only || backup_failed {
                self.unreadable.leave_as_it_is(&path);
            }
            return Vec::new();
        };
        if backup_failed {
            self.unreadable.leave_as_it_is(&path);
        } else {
            self.unreadable.clear(&path);
        }
        keep_readable_hosts(&mut value, &mut keep_settings, |entry| {
            serde_json::from_value::<T>(entry.clone()).is_ok()
        });
        serde_json::from_value::<Vec<T>>(value).unwrap_or_default()
    }

    fn write_unlocked<T: Serialize + ?Sized>(&self, value: &T) -> io::Result<()> {
        if self.read_only {
            return Ok(());
        }
        let path = self.path();
        self.unreadable.check_writable(&path)?;
        let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
        write_atomic(&path, text.as_bytes(), true)
    }

    /// Reads hosts as caller-typed records (e.g. Desktop's `HostRecord`), keeping valid entries if
    /// another entry or setting is unreadable.
    pub fn load_records<T, F>(&self, keep_settings: F) -> Vec<T>
    where
        T: DeserializeOwned,
        F: FnMut(&mut Value),
    {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.read_kept_unlocked(keep_settings)
    }

    /// Saves caller-typed host records atomically to `hosts.json`.
    pub fn save_records<T: Serialize>(&self, records: &[T]) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.write_unlocked(records)
    }

    pub fn all(&self) -> Vec<HostEntry> {
        self.load_records(|_| {})
    }

    pub fn get_by_uuid(&self, uuid: &str) -> Option<HostEntry> {
        if uuid.is_empty() {
            return None;
        }
        self.all()
            .into_iter()
            .find(|entry| entry.shared.unique_id == uuid)
    }

    pub fn get_by_name(&self, name: &str) -> Option<HostEntry> {
        self.all()
            .into_iter()
            .find(|entry| entry.shared.name == name)
    }

    /// Inserts or updates a host by `shared.unique_id`. Returns `Ok(true)` and writes to disk ONLY
    /// when the entry is new or its persistent fields changed. Preserves Desktop-owned
    /// `SharedHostFields`, `pinned_endpoint`, `alias`, and `extra` when incoming leaves them empty.
    pub fn upsert(&self, entry: HostEntry) -> io::Result<bool> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut entries: Vec<HostEntry> = self.read_kept_unlocked(|_| {});
        if !entry.shared.unique_id.is_empty()
            && let Some(existing) = entries
                .iter_mut()
                .find(|e| e.shared.unique_id == entry.shared.unique_id)
        {
            let merged = merge_preserving_desktop(existing, entry);
            if *existing == merged {
                return Ok(false);
            }
            *existing = merged;
            self.write_unlocked(&entries)?;
            return Ok(true);
        }
        entries.push(entry);
        self.write_unlocked(&entries)?;
        Ok(true)
    }

    /// Authoritatively replaces a host by `shared.unique_id` (or inserts if new), honoring explicit
    /// `None` clears on `pinned_endpoint`, `shared.pinned_address`, `shared.alias`,
    /// `shared.server_certificate`, and `shared.internet_play` while preserving unknown `extra`
    /// JSON keys from an existing entry.
    pub fn replace_entry(&self, mut entry: HostEntry) -> io::Result<bool> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut entries: Vec<HostEntry> = self.read_kept_unlocked(|_| {});
        if !entry.shared.unique_id.is_empty()
            && let Some(existing) = entries
                .iter_mut()
                .find(|e| e.shared.unique_id == entry.shared.unique_id)
        {
            for (key, value) in &existing.extra {
                entry
                    .extra
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            if *existing == entry {
                return Ok(false);
            }
            *existing = entry;
            self.write_unlocked(&entries)?;
            return Ok(true);
        }
        entries.push(entry);
        self.write_unlocked(&entries)?;
        Ok(true)
    }

    pub fn delete_by_uuid(&self, uuid: &str) -> io::Result<bool> {
        if uuid.is_empty() {
            return Ok(false);
        }
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut entries: Vec<HostEntry> = self.read_kept_unlocked(|_| {});
        let before = entries.len();
        entries.retain(|e| e.shared.unique_id != uuid);
        if entries.len() == before {
            return Ok(false);
        }
        self.write_unlocked(&entries)?;
        Ok(true)
    }

    pub fn has_migrated_legacy(&self) -> bool {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.marker_path().exists()
    }

    /// Imports legacy SQLite rows at most once per directory (guarded by `.hosts_migrated`) so
    /// hosts deleted later are never resurrected from untouched legacy databases.
    pub fn import_legacy_if_needed(&self, legacy_hosts: Vec<HostEntry>) -> io::Result<bool> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let marker = self.marker_path();
        if marker.exists() {
            return Ok(false);
        }
        if self.read_only {
            return Ok(false);
        }
        let path = self.path();
        if path.exists() {
            let text =
                fs::read_to_string(&path).inspect_err(|_| self.unreadable.leave_as_it_is(&path))?;
            if serde_json::from_str::<Value>(&text).is_err() {
                let _ = back_up(&path, "bad");
                self.unreadable.leave_as_it_is(&path);
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} is unreadable; refusing to finalize migration", path.display()),
                ));
            }
        }
        let mut entries: Vec<HostEntry> = self.read_kept_unlocked(|_| {});
        self.unreadable.check_writable(&path)?;
        let mut changed = false;
        for host in legacy_hosts {
            if host.shared.unique_id.is_empty()
                || entries
                    .iter()
                    .any(|e| e.shared.unique_id == host.shared.unique_id)
            {
                continue;
            }
            entries.push(host);
            changed = true;
        }
        if changed || !path.exists() {
            self.write_unlocked(&entries)?;
        }
        write_atomic(&marker, b"1\n", true)?;
        Ok(changed)
    }
}
