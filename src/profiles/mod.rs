use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::platform::DeviceSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppOverride {
    priority: usize,
    process_path: PathBuf,
    override_set: DeviceSet,
}
