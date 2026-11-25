//! Per-PC/per-game setting overrides, named profiles, and untrusted scan-sync displays.

use serde::{Deserialize, Serialize};

use super::labels;
use super::migrate::{fold_lower_bitrate, fold_tell_host, normalize_values};
use super::Settings;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Overrides(pub(super) serde_json::Map<String, serde_json::Value>);

impl<'de> Deserialize<'de> for Overrides {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut values = serde_json::Map::deserialize(deserializer)?;
        serde_json::from_value::<Settings>(serde_json::Value::Object(values.clone()))
            .map_err(serde::de::Error::custom)?;
        normalize_values(&mut values);
        fold_lower_bitrate(&mut values);
        fold_tell_host(&mut values);
        Ok(Self(values))
    }
}

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn has(&self, keys: &[&str]) -> bool {
        keys.iter().any(|key| self.0.contains_key(*key))
    }

    pub fn capture(&mut self, settings: &Settings, keys: &[&str]) {
        let values = serde_json::to_value(settings).expect("settings serialize");
        for key in keys {
            self.0.insert((*key).to_owned(), values[*key].clone());
        }
    }

    pub fn clear(&mut self, keys: &[&str]) {
        for key in keys {
            self.0.remove(*key);
        }
    }

    pub fn apply(&self, base: &Settings) -> Settings {
        let mut values = serde_json::to_value(base).expect("settings serialize");
        let obj = values.as_object_mut().unwrap();
        obj.extend(self.0.clone());
        // Resolution spans `width`, `height`, and `native_scale`: pre-6.4.1 layers that set pixel
        // dimensions without `native_scale` must keep full 100% scale rather than inheriting main's.
        if (self.0.contains_key("width") || self.0.contains_key("height"))
            && !self.0.contains_key("native_scale")
        {
            obj.insert("native_scale".to_owned(), serde_json::json!(100));
        }
        // A value this version cannot read is left out with the base's own, not a panic on the
        // UI thread: a file from a newer Nightjar, or a hand edit, costs its entry alone.
        serde_json::from_value(values).unwrap_or_else(|e| {
            eprintln!("nightjar: ignoring settings overrides this version cannot read: {e}");
            base.clone()
        })
    }

    pub fn needs_gpu(&self) -> bool {
        self.apply(&Settings::default()).needs_gpu()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: u64,
    pub name: String,
    pub settings: Overrides,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profiles {
    pub active: Option<u64>,
    pub entries: Vec<Profile>,
    pub automatic: bool,
    pub mobile: Option<u64>,
    pub home: Option<u64>,
    pub wifi: Option<u64>,
    pub home_wifi: Vec<String>,
    pub mobile_limit_kbps: u32,
}

impl Profiles {
    pub fn selected(&self, network: crate::network_types::Kind) -> Option<u64> {
        let rule = if self.automatic {
            match network {
                crate::network_types::Kind::Mobile => self.mobile,
                crate::network_types::Kind::HomeWifi => self.home,
                crate::network_types::Kind::Wifi => self.wifi,
                crate::network_types::Kind::Other => None,
            }
        } else {
            None
        };
        rule.or(self.active)
    }

    pub fn settings(&self, base: &Settings, network: crate::network_types::Kind) -> Settings {
        self.profile(network)
            .map_or_else(|| base.clone(), |p| p.settings.apply(base))
    }

    /// The profile a stream on `network` uses, None for the main settings.
    pub fn profile(&self, network: crate::network_types::Kind) -> Option<&Profile> {
        self.entries.iter().find(|p| Some(p.id) == self.selected(network))
    }

    pub fn bitrate_limit(&self, network: crate::network_types::Kind) -> u32 {
        if network == crate::network_types::Kind::Mobile {
            self.mobile_limit_kbps
        } else {
            0
        }
    }

    pub fn check_name(&self, name: &str, except: Option<u64>) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 32 {
            return Err(labels::PROFILE_NAME_LEN);
        }
        if self
            .entries
            .iter()
            .any(|p| Some(p.id) != except && p.name.to_lowercase() == name.to_lowercase())
        {
            return Err(labels::PROFILE_NAME_TAKEN);
        }
        Ok(())
    }

    pub fn create(&mut self, name: &str, settings: Overrides) -> Result<u64, &'static str> {
        self.check_name(name, None)?;
        if self.entries.len() >= 50 {
            return Err(labels::PROFILE_LIMIT);
        }
        let id = self
            .entries
            .iter()
            .map(|p| p.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(labels::PROFILE_NO_IDS)?;
        self.entries.push(Profile { id, name: name.trim().to_owned(), settings });
        Ok(id)
    }

    pub fn delete(&mut self, id: u64) {
        self.entries.retain(|p| p.id != id);
        if self.active == Some(id) {
            self.active = None;
        }
        for rule in [&mut self.mobile, &mut self.home, &mut self.wifi] {
            if *rule == Some(id) {
                *rule = None;
            }
        }
    }
}

/// The displays whose presents a stream found not starting their scans (`stream::scan_sync`),
/// by the monitor's device interface path (`\\?\DISPLAY#...#{...}`, its hardware ID and instance,
/// rather than the output name `\.\DISPLAY1` that another monitor can take over), kept in
/// `scan_untrusted.json`: Whole frames under VRR Automatic starts on Windows V-Sync on them from
/// the next run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanUntrusted {
    pub displays: Vec<String>,
}
