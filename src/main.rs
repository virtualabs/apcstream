use midir::{MidiOutput, MidiOutputPort, MidiOutputConnection, MidiInputPort, MidiInput};
use anyhow::Result;
use obws::Client;
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use serde_yaml;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct ButtonConfig {
    #[serde(rename = "buttons")]
    buttons: HashMap<u8, String>,
}


fn led_on(apcmini: &mut MidiOutputConnection, led: u8) {
    const NOTE_ON_MSG: u8 = 0x90;
    const VELOCITY: u8 = 0x03;

    let _ = apcmini.send(&[NOTE_ON_MSG, led, VELOCITY]);
}

fn led_off(apcmini: &mut MidiOutputConnection, led: u8) {
    const NOTE_ON_MSG: u8 = 0x90;
    const VELOCITY: u8 = 0x00;

    let _ = apcmini.send(&[NOTE_ON_MSG, led, VELOCITY]);
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

#[tokio::main]
async fn main() -> Result<()> {
    let mut current_scene: Option<u8> = None;

    let (tx, mut rx) = mpsc::channel(32);

    // Load config file (config.yaml)
    let config: ButtonConfig = serde_yaml::from_reader(std::fs::File::open("config.yaml")?)?;
    println!("config: {:?}", config);

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

    // Connect to our MIDI controller (for input)
    let input_conn = midi_in.connect(&apcmini_in.unwrap(), "APCMini", move |stamp, message, _| {
            let tx2 = tx.clone();
            //println!("received MIDI msg: {message:02x?}");
            let _ = tx2.blocking_send(message.to_vec());
    }, ()).unwrap();
    
    // Connect to the OBS instance through obs-websocket.
    let client = Client::connect("localhost", 4445).await?;

    /* Main loop, process messages. */
    while let Some(message) = rx.recv().await {
        match (message[0], message[1]) {
            (0x90, 0...64) => {
                let scene = config.buttons.get(&message[1]);
                if current_scene.is_none() {
                    current_scene = Some(message[1]);
                } else {
                    led_off(&mut apcout, current_scene.unwrap());
                    current_scene = Some(message[1]);
                }
                if let Some(id) = scene {
                    client.scenes().set_current_scene(id).await?;
                    led_on(&mut apcout, message[1]);
                }
            },
            _ => {}
        }
    }
    
    Ok(())
}
