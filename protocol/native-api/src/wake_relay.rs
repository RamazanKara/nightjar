//! Away wake relay (`feature::WAKE_RELAY`): an always-on paired device on the PC's network
//! wakes the PC for a paired device that is away, over `rubylight-traverse`
//! (`docs/designs/15-away-wake-relay.md`).
//!
//! - `GET /wake/relay` (`launch` or Host control): the PC's relays and, for a caller with
//!   `launch`, its [`WakeRelayBundle`], which it keeps to ask a relay from away.
//! - `POST /wake/relay/enroll` (Host control): the caller enrolls itself as a relay, refreshes
//!   its [`RelayAssignment`], leaves, revokes another relay or rotates the PC's wake keys. Only
//!   a caller on one of the PC's own private subnets enrolls or refreshes.
//!
//! Secrets travel only here, over the paired HTTPS connection, as 64 hex digits.
use serde::{Deserialize, Serialize};

/// A relay the PC enrolled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayPeer {
    /// The relay's device id: the SHA-256 of its certificate, 64 hex digits.
    pub id: String,
    pub name: String,
    pub enrolled_unix: u64,
    /// When the relay last refreshed its assignment; `0` when never since enrolling.
    #[serde(default)]
    pub last_seen_unix: u64,
}

/// What a device needs to ask one relay for a wake.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayCredential {
    pub id: String,
    pub name: String,
    /// The device's wake secret for this relay, 64 hex digits.
    pub secret: String,
}

/// A device's away wake access to this PC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeRelayBundle {
    /// The PC's unique id, as `/serverinfo` has it.
    pub target_id: String,
    pub epoch: u64,
    /// The caller's device id, 64 hex digits.
    pub device: String,
    pub relays: Vec<RelayCredential>,
}

/// `GET /wake/relay`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeRelayInfo {
    /// The host's Away wake relay setting is on (with Play over the internet).
    pub enabled: bool,
    pub target_id: String,
    pub epoch: u64,
    #[serde(default)]
    pub relays: Vec<RelayPeer>,
    /// The caller's access; `null` without `launch`, while the setting is off, or before a relay
    /// enrolled.
    #[serde(default)]
    pub bundle: Option<WakeRelayBundle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
pub enum RelayEnroll {
    /// Enroll the caller as a relay, or refresh its assignment; `name` is how the PC lists it.
    Enroll {
        #[serde(default)]
        name: Option<String>,
    },
    /// Fetch the caller's assignment again, as a relay does every `refresh_seconds`. Unlike
    /// `enroll` it never enrolls: a relay removed on the PC gets `not_found` and stops.
    Refresh,
    /// The caller stops being a relay.
    Leave,
    /// Remove another relay by id.
    Revoke { relay: String },
    /// New keys for every relay and device: each device's access works again once it next
    /// reads `GET /wake/relay`, each relay's once it next enrolls.
    RotateEpoch,
}

/// What a relay needs to wake the PC, for the devices the PC lets wake it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayAssignment {
    pub target_id: String,
    pub target_name: String,
    pub epoch: u64,
    /// The relay's secret, 64 hex digits.
    pub secret: String,
    /// The PC's wake MACs, `aa:bb:cc:dd:ee:ff`.
    pub macs: Vec<String>,
    /// The broadcasts of the PC's subnets.
    pub broadcasts: Vec<String>,
    /// The PC's IPv4 address on the relay's network, to see it come up.
    #[serde(default)]
    pub address: Option<String>,
    pub http_port: u16,
    /// Device ids (64 hex digits) of the devices that may wake the PC.
    pub devices: Vec<String>,
    /// Enroll again within this many seconds, so a device the PC drops loses its access.
    pub refresh_seconds: u32,
}

/// `POST /wake/relay/enroll`: the caller's assignment after `enroll`; `null` after the others.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayEnrolled {
    #[serde(default)]
    pub assignment: Option<RelayAssignment>,
    #[serde(default)]
    pub relays: Vec<RelayPeer>,
}
