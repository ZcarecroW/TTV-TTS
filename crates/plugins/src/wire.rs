//! What the app and the proxy exchange about plugins: the chains the app
//! set up (with the plugins' settings) and what the voice host PC has.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::PluginInfo;

/// One effect of the chain on every voice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub id: String,
    pub plugin: PluginInfo,
    pub enabled: bool,
    /// The plugin's settings (base64), as it saved them.
    #[serde(default)]
    pub state: Option<String>,
}

/// A plugin effect viewers can get (one plugin with its own settings).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    pub name: String,
    pub plugin: PluginInfo,
    pub enabled: bool,
    /// How often it's picked compared with the others (1–5).
    pub weight: u32,
    #[serde(default)]
    pub state: Option<String>,
}

/// `PUT /ttv/v1/plugins/chains`: everything the app runs on speech.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chains {
    pub main: Vec<Slot>,
    pub variants: Vec<Variant>,
}

/// `GET /ttv/v1/plugins`: the voice host PC's plugins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HostView {
    /// Installed effects (None: not scanned yet).
    pub installed: Option<Vec<PluginInfo>>,
    pub scanning: bool,
    /// Slot and variant ids of the app's chains that this PC doesn't have.
    pub missing: Vec<String>,
    /// Slot id -> why it doesn't load here.
    pub errors: HashMap<String, String>,
    /// The chains as the app sent them (without settings).
    pub chains: Chains,
    /// Editors can open on this PC.
    pub editors: bool,
}

impl Chains {
    /// The same chains without the settings (small, for views).
    pub fn without_states(&self) -> Self {
        Self {
            main: self.main.iter().map(|s| Slot { state: None, ..s.clone() }).collect(),
            variants: self.variants.iter().map(|v| Variant { state: None, ..v.clone() }).collect(),
        }
    }
}
