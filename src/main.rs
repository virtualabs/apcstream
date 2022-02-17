use anyhow::Result;
use obws::{Client, requests::{Volume, SceneItemRender}};
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

fn apcmini_init(apcmini: &mut APCMini, config: &DeckConfig, audio: &mut HashMap<u8,AudioSourceState>) -> Result<()> {
    // Set scenes buttons
    for (btnid, _) in config.scenes.iter() {
        apcmini.set_led( *btnid, LedState::Red);
    }

    // Set audio UI and state
    for (volid, _) in config.audio.iter() {
        apcmini.set_led( volid+64, LedState::Off);
        audio.insert(*volid, AudioSourceState::Active);
    }

    // Set reaction buttons
    for (btnid, _) in config.reactions.iter() {
        apcmini.set_led( *btnid, LedState::Yellow);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut current_scene: Option<u8> = None;
    let mut apc_audio_states = HashMap::new();

    let (tx, mut rx) = mpsc::channel(32);

    let config = load_deck_config()?;

    // Build our reverse scene lookup
    let mut rev_scene_lookup = HashMap::new();
    for (scene, name) in config.scenes.iter() {
        rev_scene_lookup.insert(name, *scene);
    }

    // Connect to our APC Mini.
    let mut apcmini = APCMini::new(tx)?;

    // Reset all leds
    for led in 0..64 {
        apcmini.led_off(led);
    }

    // Configure leds based on config
    let _ = apcmini_init(&mut apcmini, &config, &mut apc_audio_states);

    // Connect to the OBS instance through obs-websocket.
    let client = Client::connect("localhost", 4445).await?;

    // List sources
    let sources = client.sources().get_sources_list().await?;
    let mut audio_sources = Vec::new();
    for source in sources.iter().filter(|item| {(item.type_id == "pulse_input_capture") || (item.type_id == "pulse_output_capture")}) {
        println!("Source: {:?}", source);
        audio_sources.push(&source.name);
    }

    // Retrieve current scene
    match client.scenes().get_current_scene().await {
        Ok(scene) => {
            let scene_id = rev_scene_lookup.get(&scene.name);
            if let Some(id) = scene_id {
                current_scene = Some(*id);
                apcmini.set_led(*id, LedState::Green);
            }
        },
        Err(_) => {
            current_scene = None;
        }
    }

    /* Hide all reactions. */
    for (_, reaction_name) in config.reactions.iter() {
        let hide_reaction = SceneItemRender {
            scene_name : None,
            source: reaction_name,
            item: None,
            render: false
        };
        let _ = client.scene_items().set_scene_item_render(hide_reaction).await;
    }

    /* Main loop, process messages. */
    while let Some(message) = rx.recv().await {
        match (message[0], message[1]) {
            (0x90, 0...63) => {
                let scene = config.scenes.get(&message[1]);
                if scene.is_some() {
                    if current_scene.is_none() {
                        current_scene = Some(message[1]);
                    } else {
                        apcmini.set_led(current_scene.unwrap(), LedState::Red);
                        current_scene = Some(message[1]);
                    }

                    /* Hide all reactions. */
                    for (_, reaction_name) in config.reactions.iter() {
                        let hide_reaction = SceneItemRender {
                            scene_name : None,
                            source: reaction_name,
                            item: None,
                            render: false
                        };
                        let _ = client.scene_items().set_scene_item_render(hide_reaction).await;
                    }

                    client.scenes().set_current_scene(scene.unwrap()).await?;
                    apcmini.set_led( message[1], LedState::Green);
                } else {
                    /* Check if it corresponds to a known reaction. */
                    let reaction = config.reactions.get(&message[1]);
                    if let Some(reaction_name) = reaction {
                        println!("reaction: {:}", reaction_name);
                        let _ = client.media_control().restart_media(reaction_name).await;
                        let show_reaction = SceneItemRender {
                            scene_name : None,
                            source: reaction_name,
                            item: None,
                            render: true
                        };
                        let _ = client.scene_items().set_scene_item_render(show_reaction).await;
                    }
                }
            },
            (0x90, 64...71) => {
                let audio_index = message[1] - 64;
                let audio_source = config.audio.get(&audio_index);
                if audio_source.is_some() {
                    /* Check if source is muted */
                    if let Some(state) = apc_audio_states.get(&(message[1] - 64)) {
                        /* Tell OBS source is muted. */
                        client.sources().toggle_mute(audio_source.unwrap()).await?;

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
                let audio_source = config.audio.get(&(message[1]-48));
                if let Some(name) = audio_source {
                    let volume = Volume {
                        source : name,
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
