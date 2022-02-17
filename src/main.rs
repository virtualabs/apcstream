use midir::{MidiOutput, MidiOutputPort, MidiOutputConnection, MidiInputPort, MidiInput};
use anyhow::Result;
use obws::{Client, responses::SourceListItem, requests::{Volume, SceneItemRender}};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use serde_yaml;
use std::collections::HashMap;

const LED_OFF: u8 = 0x00;
const LED_GREEN: u8 = 0x01;
const LED_RED: u8 = 0x03;
const LED_YELLOW: u8 = 0x05;

#[derive(Serialize, Deserialize, Debug)]
pub struct DeckConfig {
    #[serde(rename = "scenes")]
    scenes: HashMap<u8, String>,
    #[serde(rename = "audio")]
    audio: HashMap<u8, String>,
    #[serde(rename = "reactions")]
    reactions: HashMap<u8, String>
}

enum AudioSourceState {
    Muted,
    Active
}

fn set_led(apcmini: &mut MidiOutputConnection, led: u8, led_state: u8, blink: Option<bool>) {
    const NOTE_ON_MSG: u8 = 0x90;

    // If led is not off and blink required
    if let Some(b) = blink {
        if b && (led_state>0) {
            // Increments led_state (velocity in our MIDI message)
            let led_state = led_state + 1;
        }
    }
    let _ = apcmini.send(&[NOTE_ON_MSG, led, led_state]);
}

fn led_off(apcmini: &mut MidiOutputConnection, led: u8) {
    set_led(apcmini, led, LED_OFF, None);
}

// Find APC Mini MIDI output
fn find_apcmini_output(midi_out: &MidiOutput) -> Option<MidiOutputPort> {
    let out_ports = midi_out.ports();
    let apcmini: Option<MidiOutputPort> = match out_ports.len() {
        0 => {
            println!("no output port found");
            None
        },
        _ => {
            let mut found: Option<MidiOutputPort> = None;
            println!("\nAvailable output ports:");
            for (i, p) in out_ports.into_iter().enumerate() {
                println!("{}: {}", i, midi_out.port_name(&p).unwrap());
                if midi_out.port_name(&p).unwrap().find("APC MINI").is_some() {
                    found = Some(p);
                    break;
                }
            }
            found
        }
    };
    apcmini
}

fn find_apcmini_input(midi_in: &MidiInput) -> Option<MidiInputPort> {
    let in_ports = midi_in.ports();
    let apcmini: Option<MidiInputPort> = match in_ports.len() {
        0 => {
            println!("no input port found");
            None
        },
        _ => {
            let mut found: Option<MidiInputPort> = None;
            println!("\nAvailable input ports:");
            for (i, p) in in_ports.into_iter().enumerate() {
                println!("{}: {}", i, midi_in.port_name(&p).unwrap());
                if midi_in.port_name(&p).unwrap().find("APC MINI").is_some() {
                    found = Some(p);
                    break;
                }
            }
            found
        }
    };
    apcmini
}

fn apcmini_init(apcmini: &mut MidiOutputConnection, config: &DeckConfig, audio: &mut HashMap<u8,AudioSourceState>) -> Result<()> {
    // Set scenes buttons
    for (btnid, _) in config.scenes.iter() {
        set_led(apcmini, *btnid, LED_RED, None);
    }

    // Set audio UI and state
    for (volid, _) in config.audio.iter() {
        set_led(apcmini, volid+64, LED_OFF,None);
        audio.insert(*volid, AudioSourceState::Active);
    }

    // Set reaction buttons
    for (btnid, _) in config.reactions.iter() {
        set_led(apcmini, *btnid, LED_YELLOW, None);
    }

    Ok(())
}

async fn hide_reactions(client: Client, scene: Option<&str>, reaction: &str) -> Result<()> {
    let hide_reaction = SceneItemRender {
        scene_name : scene,
        source: reaction,
        item: None,
        render: false
    };
    client.scene_items().set_scene_item_render(hide_reaction).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut current_scene: Option<u8> = None;
    let mut apc_audio_states = HashMap::new();

    let (tx, mut rx) = mpsc::channel(32);

    // Load config file (config.yaml)
    let config: DeckConfig = serde_yaml::from_reader(std::fs::File::open("config.yaml")?)?;
    println!("config: {:?}", config);

    // Build our reverse scene lookup
    let mut rev_scene_lookup = HashMap::new();
    for (scene, name) in config.scenes.iter() {
        rev_scene_lookup.insert(name, *scene);
    }

    let midi_out = match MidiOutput::new("My Test Output") {
        Ok(midi_out) => midi_out,
        Err(_) => {
            println!("Cannot find a suitable MIDI device");
            return Ok(());
        }
    };

    let midi_in = match MidiInput::new("My test input") {
        Ok(midi_in) => midi_in,
        Err(_) => {
            println!("Cannot find a suitable MIDI device");
            return Ok(());
        }
    };

    // Get an output port (read from console if multiple are available)
    let apcmini_out: Option<MidiOutputPort> = find_apcmini_output(&midi_out);
    let apcmini_in: Option<MidiInputPort> = find_apcmini_input(&midi_in);

    // Connect to our MIDI controller (for output)
    let mut apcout =  midi_out.connect(&apcmini_out.unwrap(), "APCMini").expect("OOps");

    // Reset all leds
    for led in 0..64 {
        led_off(&mut apcout, led);
    }

    // Configure leds based on config
    let _ = apcmini_init(&mut apcout, &config, &mut apc_audio_states);

    // Connect to our MIDI controller (for input)
    let input_conn = midi_in.connect(&apcmini_in.unwrap(), "APCMini", move |stamp, message, _| {
            let tx2 = tx.clone();
            //println!("received MIDI msg: {message:02x?}");
            let _ = tx2.blocking_send(message.to_vec());
    }, ()).unwrap();
    
    // Connect to the OBS instance through obs-websocket.
    let client = Client::connect("localhost", 4445).await?;

    // List sources
    let mut sources = client.sources().get_sources_list().await?;
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
                set_led(&mut apcout, *id, LED_GREEN, None);
            }
        },
        Err(_) => {
            current_scene = None;
        }
    }

    /* Hide all reactions. */
    for (react_btn, reaction_name) in config.reactions.iter() {
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
                        set_led(&mut apcout, current_scene.unwrap(), LED_RED, None);
                        current_scene = Some(message[1]);
                    }

                    /* Hide all reactions. */
                    for (react_btn, reaction_name) in config.reactions.iter() {
                        let hide_reaction = SceneItemRender {
                            scene_name : None,
                            source: reaction_name,
                            item: None,
                            render: false
                        };
                        let _ = client.scene_items().set_scene_item_render(hide_reaction).await;
                    }

                    client.scenes().set_current_scene(scene.unwrap()).await?;
                    set_led(&mut apcout, message[1], LED_GREEN, None);
                } else {
                    /* Check if it corresponds to a known reaction. */
                    let reaction = config.reactions.get(&message[1]);
                    if let Some(reaction_name) = reaction {
                        println!("reaction: {:}", reaction_name);
                        let result = client.media_control().restart_media(reaction_name).await;
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
                                set_led(&mut apcout, audio_index + 64, LED_RED, None);
                            },
                            AudioSourceState::Muted => {
                                /* Mark as active. */
                                apc_audio_states.insert(audio_index, AudioSourceState::Active);
                                set_led(&mut apcout, audio_index + 64, LED_OFF, None);
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
