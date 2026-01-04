use std::sync::mpsc::{self, Receiver, Sender};

use shadowplay::ShadowPlayActor;
use tracing::*;

use crate::errors::AppResult;

#[derive(Debug)]
pub enum ShadowPlayCommand {
    ChangeMicrophone(String),
}

#[derive(Debug)]
pub struct ShadowPlayHandle {
    command_tx: Sender<ShadowPlayCommand>,
}

impl ShadowPlayHandle {
    pub fn build() -> AppResult<Self> {
        let actor = ShadowPlayActor::build()?;
        let (command_tx, command_rx) = mpsc::channel();
        std::thread::spawn(move || {
            shadowplay_actor_loop(actor, command_rx);
        });

        Ok(Self { command_tx })
    }
    pub fn microphone_change(&self, desired_guid: &str) {
        _ = self
            .command_tx
            .send(ShadowPlayCommand::ChangeMicrophone(desired_guid.to_owned()));
    }
}

fn shadowplay_actor_loop(
    actor: ShadowPlayActor,
    command_rx: Receiver<ShadowPlayCommand>,
    // event_proxy: AppEventProxy,
) {
    while let Ok(command) = command_rx.recv() {
        match command {
            ShadowPlayCommand::ChangeMicrophone(guid) => {
                if let Err(e) = actor.microphone_change(&guid) {
                    // Just silently log the error for now.
                    error!("{e}");
                };
            }
        }
    }
}
