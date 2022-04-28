use anyhow::Result;
use obws::{Client};
use tokio::sync::mpsc;
use std::{thread, time};

// Load APC Mini controller.
mod apcmini;
use apcmini::{APCMini};

// Load configuration helpers.
mod config;
use config::{load_deck_config};

mod control;
use control::Controller;


async fn obsws_connect() -> Result<Client> {
   // Connect to the OBS instance through obs-websocket.
   let client = Client::connect("localhost", 4445).await?;
   Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
  

  /* Daemon keeps running and try to connect to remote service. */
  loop {

    let (tx, mut rx) = mpsc::channel(32);
    let config = load_deck_config()?;
    let apcmini = APCMini::new(tx)?; 

    /* Connect to the OBS instance through obs-websocket. */
    if let Ok(client) = obsws_connect().await {
      let mut controller = Controller::new(apcmini, config, client).await?;
      
      /* Main loop, process messages. */
      while let Some(message) = rx.recv().await {
          match (message[0], message[1]) {
              (0x90, 0...63) => {
                if controller.on_button_press(message[1]).await.is_err() {
                  break;
                }
              },

              (0x90, 64...71) => {
                if controller.on_slider_btn_press(message[1]).await.is_err() {
                  break;
                }
              },

              (0xB0, 48...57) => {
                if controller.on_slider_change(message[1]-48, message[2]).await.is_err() {
                  break;
                }
              },
              _ => {}
          }
      }

      println!("[!] Exited main loop (server dropped connection)");
    } else {
      println!("[!] Cannot connect to OBS Websocket");
      thread::sleep(time::Duration::from_secs(1));
    }
  }
}
