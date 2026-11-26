//! Unified host registry (`hosts.json`), per-host metadata (`host_meta.json`), and client
//! pairing identity (`identity.json`) persistence shared across Desktop and Android.

pub mod applist;
pub mod client_pool;
pub mod identity;
pub mod io;
pub mod manager;
mod manager_ops;
mod manager_poll;
pub mod meta;
pub mod pairing;
pub mod poll;
pub mod power;
pub mod store;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_regressions;

pub use identity::{IdentityStore, LoadedIdentity};
pub use meta::{HostMetaEntry, HostMetaStore};
pub use nightjar_policy::hosts::{
    ALL_PACKED_MODES, ALIAS_MAX_CHARS, EndpointSlot, HostCapabilitiesSnapshot, HostNames,
    HostNotice, HostNoticesState, HostPackedEntry, HostPackedModesState, IdentityFile,
    NOTICE_FRAME_WARP_UNAVAILABLE, NOTICE_PACKED_UNAVAILABLE, NOTICE_PYROWAVE_FALLBACK,
    PinnedAddress, SharedHostFields, keep_readable_hosts,
};
pub use store::{HostEntry, HostStore};
