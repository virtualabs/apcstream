//! Streamdeck config management

use serde::{Serialize, Deserialize};
use serde_yaml;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct DeckConfig {
    #[serde(rename = "scenes")]
    pub scenes: HashMap<u8, String>,
    #[serde(rename = "audio")]
    pub audio: HashMap<u8, String>,
    #[serde(rename = "reactions")]
    pub reactions: HashMap<u8, String>
}

pub fn load_deck_config() -> anyhow::Result<DeckConfig> {
    // Load config file (config.yaml)
    let config: DeckConfig = serde_yaml::from_reader(std::fs::File::open("config.yaml")?)?;
    Ok(config)
}
