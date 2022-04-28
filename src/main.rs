use anyhow::Result;
use obws::{Client};
use tokio::sync::mpsc;
use std::{thread, time};

// Load APC Mini controller.
mod apcmini;
use apcmini::{APCMini, Message};

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
  
  let (tx, mut rx) = mpsc::channel::<Message>(32);
  let config = load_deck_config()?;
  let mut apcmini = APCMini::new(tx)?; 

  /* Daemon keeps running and try to connect to remote service. */
  loop {

    /* Connect to the OBS instance through obs-websocket. */
    if let Ok(client) = obsws_connect().await {
      let mut controller = Controller::new(&mut apcmini, &config, client).await?;
      
      /* Main loop, process messages. */
      while let Some(message) = rx.recv().await {
          match message {
            
            /* Button has been pressed (switch scenes and video sources). */
            Message::Button{id} => {
              if controller.on_button_press(id).await.is_err() {
                break;
              }
            },

            /* Slider button has been pressed (handle mute/unmute). */
            Message::SliderButton{id} => {
              if controller.on_slider_btn_press(id).await.is_err() {
                break;
              }
            },

            /* Slider value has been changed (set volume). */
            Message::Slider{id,value} => {
              if controller.on_slider_change(id, value).await.is_err() {
                break;
              }
            },
          }
      }

      println!("[!] Exited main loop (server dropped connection)");
    } else {
      println!("[!] Cannot connect to OBS Websocket");
      thread::sleep(time::Duration::from_secs(1));
    }
  }
}
