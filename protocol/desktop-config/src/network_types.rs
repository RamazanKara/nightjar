//! Pure network state and pacing types shared across config, UI and network probes.

/// Whether this platform reports the active Wi-Fi SSID to Nightjar without OS location-permission
/// restrictions. macOS withholds Wi-Fi SSIDs unless Location Services permission is granted, so
/// Wi-Fi networks there match `Kind::Wifi` ("Other Wi-Fi").
pub const SSID_SUPPORTED: bool = !cfg!(target_os = "macos");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mobile,
    HomeWifi,
    Wifi,
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Network {
    pub wifi: bool,
    pub cellular: bool,
    pub metered: bool,
    pub ssid: Option<String>,
    pub link_mbps: u32,
    pub error: Option<String>,
}

impl Network {
    pub fn kind(&self, home: &[String]) -> Kind {
        if self.cellular || self.metered {
            Kind::Mobile
        } else if !self.wifi {
            Kind::Other
        } else if self.ssid.as_ref().is_some_and(|name| home.contains(name)) {
            Kind::HomeWifi
        } else {
            Kind::Wifi
        }
    }

    /// The link speed sent to the host as the pacing hint, for every codec: 0 on Wi-Fi or
    /// cellular, where the adapter speed says nothing about the path.
    pub fn wired_link_mbps(&self) -> u32 {
        if self.wifi || self.cellular {
            0
        } else {
            self.link_mbps.min(9999)
        }
    }

    /// Whether the route to the host is Wi-Fi or cellular, sent as a hint so the host paces for
    /// a wireless link from the first frame. False when wired, unknown or unreadable.
    pub fn wireless(&self) -> bool {
        self.wifi || self.cellular
    }

    pub fn description(&self, home: &[String]) -> String {
        if let Some(error) = &self.error {
            return format!("Network details unavailable: {error}");
        }
        let name = match self.kind(home) {
            Kind::Mobile => "Mobile or metered network",
            Kind::HomeWifi => "Home Wi-Fi",
            Kind::Wifi => "Other Wi-Fi",
            Kind::Other => "Ethernet or other network",
        };
        if self.wifi && self.ssid.is_none() {
            format!("{name} · Wi-Fi name unavailable")
        } else {
            name.into()
        }
    }

    /// This connection as it applies to the route to one host: the Wi-Fi flag and the link speed
    /// follow the interface that reaches the host, so a PC on the local Ethernet is wired even
    /// when Wi-Fi carries the internet. Unchanged when the route does not tell.
    ///
    /// On a wireless route the speed stays the Wi-Fi or cellular rate the system reports (what a
    /// stored pace calibration is keyed on); the hint sent to the host stays 0 through
    /// `wired_link_mbps`. With a wired internet connection the Wi-Fi name is unknown, so no
    /// Wi-Fi calibration matches.
    pub fn toward(&self, route: Result<Toward, Fallback>) -> Network {
        let Ok(route) = route else { return self.clone() };
        Network {
            wifi: route.wireless && (self.wifi || !self.cellular),
            cellular: route.wireless && self.cellular,
            link_mbps: match (route.wireless, self.wifi || self.cellular) {
                (false, _) => route.link_mbps,
                (true, true) => self.link_mbps,
                (true, false) => 0,
            },
            ssid: if route.wireless { self.ssid.clone() } else { None },
            error: None,
            ..self.clone()
        }
    }
}

/// What the interface carrying the traffic to a host is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Medium {
    Wired,
    Wireless,
    Other,
}

/// The route to one host: whether it leaves over Wi-Fi or cellular, and the speed of the port when
/// it is wired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toward {
    pub wireless: bool,
    pub link_mbps: u32,
}

impl Toward {
    /// Tunnels, loopback and anything else that does not say what carries the traffic are `None`,
    /// so the caller falls back to the internet connection.
    pub fn of(medium: Medium, link_mbps: u32) -> Option<Toward> {
        match medium {
            Medium::Wired => Some(Toward { wireless: false, link_mbps }),
            Medium::Wireless => Some(Toward { wireless: true, link_mbps: 0 }),
            Medium::Other => None,
        }
    }
}

/// Why `toward` has no answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// The host name did not resolve to an address.
    Resolve,
    /// The system has no route to the address, or its interface could not be read.
    Route,
    /// The interface is loopback, a tunnel, virtual or of a type that says nothing.
    NotTelling,
}

impl std::fmt::Display for Fallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Fallback::Resolve => "host name did not resolve",
            Fallback::Route => "route lookup failed",
            Fallback::NotTelling => "interface does not say",
        })
    }
}

/// How often the launcher's network watcher reads the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// A page shows the link speed or starts a test that keeps it.
    Live,
    /// Only what kind of network it is matters (which profile applies).
    Idle,
    /// A stream is up, with a watcher of its own.
    Paused,
}
