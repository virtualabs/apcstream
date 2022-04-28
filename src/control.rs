//! Streamdeck config management

use obws::{Client,requests::{Volume, SceneItemRender}};
use std::collections::HashMap;
use crate::apcmini::{APCMini, LedState};
use crate::config::DeckConfig;


enum AudioSourceState {
  Muted,
  Active
}

#[derive(PartialEq)]
enum VideoSourceState {
  Visible,
  Hidden
}


pub struct Controller<'a> {
  apcmini: &'a mut APCMini,
  config: &'a DeckConfig,
  client: Client,
  audio_sources: Vec<String>,
  audio_states: HashMap<u8, AudioSourceState>,
  video_sources: Vec<String>,
  video_states: HashMap<u8, VideoSourceState>,
  current_scene: Option<u8>
}

impl<'a> Controller<'a> {

  /**
   * Creates a controller instance.
   **/

  pub async fn new(apcmini:&'a mut APCMini, config: &'a DeckConfig, client: Client) -> anyhow::Result<Controller<'a>> {
    
    /* Retrieve audio  sources from OBS websocket client. */
    let mut audio_sources = Vec::new();
    let mut video_sources = Vec::new();
    let sources = client.sources().get_sources_list().await?;

    /* Retrieve audio sources from OBS websocket client. */
    for source in sources.iter().filter(|item| {(item.type_id == "pulse_input_capture") || (item.type_id == "pulse_output_capture")}) {
      println!("Source: {:?}", source);
      audio_sources.push(source.name.clone());
    }

    /* Retrieve video sources from OBS websocket client. */
    for source in sources.iter().filter(|item| {item.type_id == "v4l2_input"}) {
      println!("Source: {:?}", source);
      video_sources.push(source.name.clone());
    }

    /* Initialize keyboard. */
    for led in 0..64 {
      apcmini.led_off(led);
    }

    for binding in config.bindings.scenes.iter() {
      apcmini.set_led( binding.button, LedState::Red);
    }

    // Set audio UI and state
    let mut audio_states = HashMap::new();
    for binding in config.bindings.audio.iter() {
        apcmini.set_led( binding.slider+64, LedState::Off);
        audio_states.insert(binding.slider, AudioSourceState::Active);
    }

    let mut video_states= HashMap::new();
    for binding in config.bindings.sources.iter() {
      apcmini.set_led(binding.button, LedState::Yellow);
      video_states.insert(binding.button, VideoSourceState::Visible);
    }

    /* Populate our structure. */
    let controller = Controller {
      apcmini: apcmini,
      config: config,
      client: client,
      audio_sources: audio_sources,
      audio_states: audio_states,
      video_sources: video_sources,
      video_states: video_states,
      current_scene: None
    };


    /* Success. */
    Ok(controller)
  }


  /**
   * Dispatch button press event.
   **/

  pub async fn on_button_press(&mut self, btn_id: u8) -> anyhow::Result<()> {
    let scene = self.config.get_scene_by_button(btn_id);
    if let Some(scene) = scene {
      self.switch_to_scene(scene, btn_id).await?;
    } else {
      if let Some(source) = self.config.get_source_by_button(btn_id) {
        self.toggle_video_source(source, btn_id).await?;
      }
    }

    Ok(())
  }


  /**
   * Dispatch slider button press event.
   **/

  pub async fn on_slider_btn_press(&mut self, btn_id: u8) -> anyhow::Result<()> {
    let slider_id = btn_id - 64;
    let audio_source = self.config.get_audiosource_by_slider(slider_id);
    if let Some(audio_source) = audio_source {
      self.toggle_audio_source(audio_source, btn_id).await?;
    }

    Ok(())
  }


  /**
   * Dispatch slider value change event.
   **/

  pub async fn on_slider_change(&mut self, slider_id: u8, slider_value: u8) -> anyhow::Result<()> {
    let audio_source = self.config.get_audiosource_by_slider(slider_id);
    if let Some(name) = audio_source {
      self.set_volume(name, slider_value).await?;
    }

    Ok(())
  }


  /**
   * Switch to a specific scene and set button light accordingly.
   **/

  pub async fn switch_to_scene(&mut self, scene: String, btn_id: u8) -> anyhow::Result<()> {
    if self.current_scene.is_none() {
      self.current_scene = Some(btn_id);
    } else {
        self.apcmini.set_led(self.current_scene.unwrap(), LedState::Red);
        self.current_scene = Some(btn_id);
    }

    /* Exit while loop on error (OBS websocket disconnected) */
    self.client.scenes().set_current_scene(&scene).await?;

    /* Update LED state. */
    self.apcmini.set_led( btn_id, LedState::Green);

    Ok(())
  }


  /**
   * Toggle video source and set button light accordingly.
   **/

  pub async fn toggle_video_source(&mut self, source: String, btn_id: u8) -> anyhow::Result<()> {
    /* If current scene is set and video_source exists. */
    if self.video_sources.contains(&&source) && self.current_scene.is_some() {

      /* Toggle video source visibility. */
      if let Some(scene_name) =  self.config.get_scene_by_button(self.current_scene.unwrap()) {
        if let Some(video_state) = self.video_states.get(&btn_id) {
          let is_visible = *video_state == VideoSourceState::Hidden;
          let scene_item = SceneItemRender {
            scene_name: Some(&scene_name),
            source: &source,
            item: None,
            render: is_visible
          };

          /* Set scene item render on OBS. */
          self.client.scene_items().set_scene_item_render(scene_item).await?;
          
          match self.video_states.get(&btn_id) {
            Some(VideoSourceState::Visible) => {
              self.video_states.insert(btn_id, VideoSourceState::Hidden);
              self.apcmini.set_led(btn_id, LedState::BlinkYellow);    
            },
            Some(&VideoSourceState::Hidden) => {
              self.video_states.insert(btn_id, VideoSourceState::Visible);    
              self.apcmini.set_led(btn_id, LedState::Yellow);
            },
            None => {
            }
          }
        }
      }
    }

    /* Success. */
    Ok(())
  }


  /**
   * Mute/unmute audio source and set slider button accordingly.
   **/

  pub async fn toggle_audio_source(&mut self, audio_source: String, btn_id: u8) -> anyhow::Result<()> {
    let audio_index = btn_id - 64;

    /* Check if source is muted */
    if let Some(state) = self.audio_states.get(&(btn_id - 64)) {

      /* Tell OBS source is muted, exit loop on error. */
      self.client.sources().toggle_mute(&audio_source).await?;

      match state {
        AudioSourceState::Active => {
            /* Mark as muted. */
            self.audio_states.insert(audio_index, AudioSourceState::Muted);
            self.apcmini.set_led(audio_index + 64, LedState::Red);
        },
        AudioSourceState::Muted => {
            /* Mark as active. */
            self.audio_states.insert(audio_index, AudioSourceState::Active);
            self.apcmini.set_led(audio_index + 64, LedState::Off);
        }
      }
    }

    /* Success. */
    Ok(())
  }


  /**
   * Set audio source volume.
   **/
   
  pub async fn set_volume(&mut self, audio_source: String, volume: u8) -> anyhow::Result<()> {
    let volume = Volume {
        source : &audio_source,
        volume : -(1.0 - f64::from(volume)/127.0)*100.0f64,
        use_decibel: Some(true)
    };

    /* Exit while loop on error. */
    self.client.sources().set_volume(volume).await?;
    
    /* Success. */
    Ok(())
  }

}