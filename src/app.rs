use std::collections::BTreeMap;

use crate::{errors::AppResult, platform::AudioNightmare, profiles::Profiles};

pub struct App {
    endpoints: AudioNightmare,
    profiles: Profiles,
}

impl App {
    pub fn build() -> AppResult<Self> {
        Ok(Self {
            endpoints: AudioNightmare::build()?,
            profiles: Profiles::build()?,
        })
    }
}
