use midir::{MidiOutput, MidiOutputPort, MidiOutputConnection};
use std::time::Duration;
use std::thread::sleep;

fn switch_led(conn: &mut MidiOutputConnection) {
    {
        // Define a new scope in which the closure `play_note` borrows conn_out, so it can be called easily
        let mut switch_led = |led: u8, enabled: bool| {
            const NOTE_ON_MSG: u8 = 0x90;
            const VELOCITY: u8 = 0x03;

            if enabled {
                // We're ignoring errors in here
                let _ = conn.send(&[NOTE_ON_MSG, led, VELOCITY]);
            } else {
                let _ = conn.send(&[NOTE_ON_MSG, led, 0x00]);
            }
        };

        for i in 0..64 {
            switch_led(i, true);
            sleep(Duration::from_millis(10));
        }
        sleep(Duration::from_millis(500));
        for i in 0..64 {
            switch_led(i, false);
            sleep(Duration::from_millis(10));
        }
    }
}

fn main() {
    let midi_out = match MidiOutput::new("My Test Output") {
        Ok(midi_out) => midi_out,
        Err(_) => {
            println!("Cannot find a suitable MIDI device");
            return;
        }
    };

    // Get an output port (read from console if multiple are available)
    let out_ports = midi_out.ports();
    let apcmini: Option<&MidiOutputPort> = match out_ports.len() {
        0 => {
            println!("no output port found");
            None
        },
        _ => {
            let mut found: Option<&MidiOutputPort> = None;
            println!("\nAvailable output ports:");
            for (i, p) in out_ports.iter().enumerate() {
                println!("{}: {}", i, midi_out.port_name(p).unwrap());
                if midi_out.port_name(p).unwrap().find("APC MINI").is_some() {
                    found = Some(p);
                    break;
                }
            }
            found
        }
    };

    // Did we find our APCMini ?
    if apcmini.is_none() {
        println!("No APC MINI found !");
    } else {        
        // Yes ! 
        println!("Found APC MINI device !");

        // Connect to our MIDI controller
        match midi_out.connect(apcmini.unwrap(), "APCMini") {
            Ok(mut conn) => switch_led(&mut conn),
            Err(_) => eprintln!("Cannot connect to APCMini.")
        };
        
    }

}
