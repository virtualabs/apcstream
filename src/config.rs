//! Streamdeck config management

use serde::{Serialize, Deserialize};
use serde_yaml;

#[derive(Serialize,Deserialize)]
pub struct AudioBinding {
  pub slider: u8,
  pub name: String,
}

#[derive(Serialize,Deserialize)]
pub struct SceneBinding {
  pub button: u8,
  pub name: String,
}

#[derive(Serialize,Deserialize)]
pub struct SourceBinding {
  pub button: u8,
  pub name: String
}

#[derive(Serialize,Deserialize)]
pub struct DeckBindings {
  pub audio: Vec<AudioBinding>,
  pub scenes: Vec<SceneBinding>,
  pub sources: Vec<SourceBinding>
}

#[derive(Serialize,Deserialize)]
pub struct DeckConfig {
  pub bindings: DeckBindings,
}

impl DeckConfig  {
  
  pub fn get_button_by_scene(&self, scene: String) -> Option<u8> {
    self.bindings.scenes.iter().find_map(
      |x| if x.name == scene {
        Some(x.button)
      } else {
        None
      }
    )
  }

  pub fn get_scene_by_button(&self, button: u8) -> Option<String> {
    self.bindings.scenes.iter().find_map(
      |x| if x.button == button {
        Some(x.name.clone())
      } else {
        None
      }
    )
  }

  /*
  pub fn get_slider_by_audiosource(&self, source: String) -> Option<u8> {
    self.bindings.audio.iter().find_map(
      |x| if x.name == source {
        Some(x.slider)
      } else {
        None
      }
    )
  }
  */

  pub fn get_audiosource_by_slider(&self, slider: u8) -> Option<String> {
    self.bindings.audio.iter().find_map(
      |x| if x.slider == slider {
        Some(x.name.clone())
      } else {
        None
      }
    )
  }

  pub fn get_source_by_button(&self, button: u8) -> Option<String> {
    self.bindings.sources.iter().find_map(
      |x| if x.button == button {
        Some(x.name.clone())
      } else {
        None
      }
    )
  }

}

pub fn load_deck_config() -> anyhow::Result<DeckConfig> {
    // Load config file (config.yaml)
    let config: DeckConfig = serde_yaml::from_reader(std::fs::File::open("config.yaml")?)?;
    Ok(config)
}
