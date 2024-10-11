use std::{path::PathBuf, time::Instant};

pub struct AudioNightmare {}

enum DeviceType {
    Playback,
    Recording,
}

// Maybe I need to have one for a detected device vs a desired device
// A desired device won't always be connected to the machine.
struct WindowsAudioDevice {
    device_type: DeviceType,
    human_name: String,
    guid: String,
}

struct DeviceSet {
    playback: WindowsAudioDevice,
    playback_comms: WindowsAudioDevice,
    recording: WindowsAudioDevice,
    recording_comms: WindowsAudioDevice,
}

struct Config {
    unify_communications_devices: bool,
    desired_set: DeviceSet,
}

struct AppOverride {
    priority: usize,
    process_path: PathBuf,
    override_set: DeviceSet,
}

struct AppContext {
    config: Config,
    overrides: Vec<AppOverride>,
    desired_set: DeviceSet,
    current_set: DeviceSet,
    // To prevent fighting with something else messing with devices
    changes_within_few_seconds: usize,
    last_change: Instant,
}
