//! The launch curtain over the native API (`docs/native-api.md`, "Launch curtain"): Show PC
//! screen tells the host, while it lists `session.launch_curtain`, that the player lifted the
//! curtain, so it stops watching the start closely and tells every device. The curtain's state
//! itself travels on the control stream (0x5548, `rubylight_protocol::launch_curtain`).

use super::native::{ACTION_READ_TIMEOUT, EMPTY_BODY, MAX_JSON_BYTES};
use super::native_api::curtain::LaunchCurtainSnapshot;
use super::{Error, HostClient};

/// The route under the native API's prefix.
const DISMISS: &str = "session/curtain/dismiss";

impl HostClient {
    /// `POST /session/curtain/dismiss` with `{}`: lifts the launch curtain for everyone and
    /// answers with its state. Needs Launch and the device that launched; another device or no
    /// session is a 409 (`not_session_owner`, `no_session`).
    pub fn dismiss_launch_curtain(&self) -> Result<LaunchCurtainSnapshot, Error> {
        self.native(DISMISS, Some(EMPTY_BODY), MAX_JSON_BYTES, ACTION_READ_TIMEOUT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::native_api::curtain::CurtainDismissRequest;
    use crate::http::native_api::PREFIX;

    #[test]
    fn dismiss_posts_the_contracts_empty_request_to_its_route() {
        assert_eq!(format!("{PREFIX}/{DISMISS}"), "/api/native/v1/session/curtain/dismiss");
        // The host refuses unknown fields: the body is the contract's request and nothing else.
        let body: CurtainDismissRequest = serde_json::from_slice(EMPTY_BODY).unwrap();
        assert_eq!(body, CurtainDismissRequest::default());
    }
}
