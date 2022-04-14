use anyhow::Result;
use obws::{Client, requests::{Volume, SceneItemRender},};
use tokio::sync::mpsc;
use std::collections::HashMap;

// Load APC Mini controller.
mod apcmini;
use apcmini::{APCMini, LedState};

// Load configuration helpers.
mod config;
use config::{DeckConfig, load_deck_config};

enum AudioSourceState {
  Muted,
  Active
}

#[derive(PartialEq)]
enum VideoSourceState {
  Visible,
  Hidden
}

fn apcmini_init(apcmini: &mut APCMini, config: &DeckConfig, audio: &mut HashMap<u8,AudioSourceState>, source: &mut HashMap<u8, VideoSourceState>) -> Result<()> {
    // Set scenes buttons
    for binding in config.bindings.scenes.iter() {
        apcmini.set_led( binding.button, LedState::Red);
    }

    // Set audio UI and state
    for binding in config.bindings.audio.iter() {
        apcmini.set_led( binding.slider+64, LedState::Off);
        audio.insert(binding.slider, AudioSourceState::Active);
    }

    for binding in config.bindings.sources.iter() {
      apcmini.set_led(binding.button, LedState::Yellow);
      source.insert(binding.button, VideoSourceState::Visible);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut current_scene: Option<u8> = None;
    let mut apc_audio_states = HashMap::new();
    let mut apc_source_states = HashMap::new();

    let (tx, mut rx) = mpsc::channel(32);

    let config = load_deck_config()?;

    // Connect to our APC Mini.
    let mut apcmini = APCMini::new(tx)?;

    // Reset all leds
    for led in 0..64 {
        apcmini.led_off(led);
    }

    // Configure leds based on config
    let _ = apcmini_init(&mut apcmini, &config, &mut apc_audio_states, &mut apc_source_states);

    // Connect to the OBS instance through obs-websocket.
    let client = Client::connect("localhost", 4445).await?;

    // List sources
    let sources = client.sources().get_sources_list().await?;
    let mut audio_sources = Vec::new();
    for source in sources.iter().filter(|item| {(item.type_id == "pulse_input_capture") || (item.type_id == "pulse_output_capture")}) {
        println!("Source: {:?}", source);
        audio_sources.push(&source.name);
    }

    let mut video_sources = Vec::new();
    for source in sources.iter().filter(|item| {item.type_id == "v4l2_input"}) {
      println!("Source: {:?}", source);
      video_sources.push(&source.name);
  }
  
    // Retrieve current scene
    match client.scenes().get_current_scene().await {
        Ok(scene) => {
            let button_id = config.get_button_by_scene(scene.name);
            if let Some(id) = button_id {
                current_scene = Some(id);
                apcmini.set_led(id, LedState::Green);
            }
        },
        Err(_) => {
            current_scene = None;
        }
    }

    /* Main loop, process messages. */
    while let Some(message) = rx.recv().await {
        match (message[0], message[1]) {
            (0x90, 0...63) => {
                let scene = config.get_scene_by_button(message[1]);
                if let Some(scene) = scene {
                    if current_scene.is_none() {
                        current_scene = Some(message[1]);
                    } else {
                        apcmini.set_led(current_scene.unwrap(), LedState::Red);
                        current_scene = Some(message[1]);
                    }

                    client.scenes().set_current_scene(&scene).await?;
                    apcmini.set_led( message[1], LedState::Green);
                } else {
                  /* TODO: refactoring ! */
                  if let Some(source) = config.get_source_by_button(message[1]) {
                    if video_sources.contains(&&source) && current_scene.is_some() {
                      if let Some(scene_name) =  config.get_scene_by_button(current_scene.unwrap()) {
                        if let Some(video_state) = apc_source_states.get(&message[1]) {
                          let is_visible = *video_state == VideoSourceState::Hidden;
                          let scene_item = SceneItemRender {
                            scene_name: Some(&scene_name),
                            source: &source,
                            item: None,
                            render: is_visible
                          };
                          client.scene_items().set_scene_item_render(scene_item).await?; 
                          match apc_source_states.get(&message[1]) {
                            Some(VideoSourceState::Visible) => {
                              apc_source_states.insert(message[1], VideoSourceState::Hidden);
                              apcmini.set_led(message[1], LedState::BlinkYellow);    
                            },
                            Some(&VideoSourceState::Hidden) => {
                              apc_source_states.insert(message[1], VideoSourceState::Visible);    
                              apcmini.set_led(message[1], LedState::Yellow);
                            },
                            None => {
                            }
                          }
                        }
                      }
                    }
                  }
                }
            },
            (0x90, 64...71) => {
                let audio_index = message[1] - 64;
                let audio_source = config.get_audiosource_by_slider(audio_index);
                if let Some(audio_source) = audio_source {
                    /* Check if source is muted */
                    if let Some(state) = apc_audio_states.get(&(message[1] - 64)) {
                        /* Tell OBS source is muted. */
                        client.sources().toggle_mute(&audio_source).await?;

                        match state {
                            AudioSourceState::Active => {
                                /* Mark as muted. */
                                apc_audio_states.insert(audio_index, AudioSourceState::Muted);
                                apcmini.set_led(audio_index + 64, LedState::Red);
                            },
                            AudioSourceState::Muted => {
                                /* Mark as active. */
                                apc_audio_states.insert(audio_index, AudioSourceState::Active);
                                apcmini.set_led(audio_index + 64, LedState::Off);
                            }
                        }
                    }
                }
            },
            (0xB0, 48...57) => {
                //println!("volume: {:?}", 20.0 * (f64::from(message[2])/127.));
                let audio_source = config.get_audiosource_by_slider(message[1]-48);
                if let Some(name) = audio_source {
                    let volume = Volume {
                        source : &name,
                        volume : -(1.0 - f64::from(message[2])/127.0)*100.0f64,
                        use_decibel: Some(true)
                    };
                    client.sources().set_volume(volume).await?;
                }
            },
            _ => {}
        }
    }
    
    Ok(())
}
