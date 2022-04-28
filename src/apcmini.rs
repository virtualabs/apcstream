//! APCMini module

use midir::{MidiOutput, MidiOutputPort, MidiOutputConnection, MidiInputConnection, MidiInputPort, MidiInput};
use tokio::sync::mpsc::Sender;

/// APCMini LED states (u8 values)
#[repr(u8)]
pub enum LedState {
    Off = 0,
    Green = 1,
    //BlinkGreen = 2,
    Red = 3,
    //BlinkRed = 4,
    Yellow = 5,
    BlinkYellow = 6
}


/// APC Mini MIDI message.
pub enum Message {
  /// Normal button has been pressed.
  Button{id:u8},

  /// Slider value has changed.
  Slider{id:u8, value:u8},

  /// Slider button has been pressed.
  SliderButton{id:u8},
}

impl Message {
  fn from_midi(message: &[u8]) -> Option<Message> {
    match message {
      [0x90, id@0..=63, ..] => Some(Message::Button{id:*id}),
      [0x90, id@64..=71, ..] => Some(Message::SliderButton{id:*id}),
      [0xB0, id@48..=57, value, ..] => Some(Message::Slider{id:*id - 48, value:*value}),
      _ => None
    }
  }
}

/// APCMini MIDI controller
pub struct APCMini {
    apc_out: MidiOutputConnection,
    apc_in: MidiInputConnection<()>
}


impl APCMini {

    pub fn new(tx: Sender<Message>) -> anyhow::Result<APCMini> {

        let midi_out = MidiOutput::new("My Test Output")?;
        let midi_in = MidiInput::new("My test input")?;
        let port_out = Self::find_apcmini_output(&midi_out).ok_or(());
        let port_in = Self::find_apcmini_input(&midi_in).ok_or(());
        let apc_out =  midi_out.connect(&port_out.unwrap(), "APCMini").expect("OOps");
        
        // Connect to our MIDI controller (for input)
        let apc_in = midi_in.connect(&port_in.unwrap(), "APCMini", move |_, message, _| {
          let parsed_msg = Message::from_midi(message);
          if let Some(msg) = parsed_msg {
            let _ = tx.blocking_send(msg);
          }
        }, ()).unwrap();

        Ok(APCMini {
            apc_out,
            apc_in
        })
    }

    // Find APC Mini MIDI output.
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

    // Find Midi input port.
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
    
    /// Set APCMini LED state.
    pub fn set_led(&mut self, led: u8, led_state: LedState) {
        const NOTE_ON_MSG: u8 = 0x90;    
        let _ = self.apc_out.send(&[NOTE_ON_MSG, led, led_state as u8]);
    }

    /// Switch off an APCMini LED.
    pub fn led_off(&mut self, led: u8) {
        self.set_led(led, LedState::Off);
    }
}
