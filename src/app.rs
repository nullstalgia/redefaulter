use std::collections::BTreeMap;

use crate::{
    errors::AppResult,
    platform::{AudioNightmare, WindowsAudioDevice},
};

pub struct App {
    nightmare: AudioNightmare,
}

impl App {
    pub fn build() -> AppResult<Self> {
        Ok(Self {
            nightmare: AudioNightmare::build()?,
        })
    }
}
