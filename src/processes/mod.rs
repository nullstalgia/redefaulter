use crate::app::{AppEventProxy, CustomEvent};
use crate::errors::{AppResult, RedefaulterError};
use crate::profiles::AppOverride;

use dashmap::DashMap;
use fs_err::{self as fs};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{collections::HashMap, sync::mpsc::Sender};
use tracing::*;
use wmi::*;

// Inspired by https://users.rust-lang.org/t/watch-for-windows-process-creation-in-rust/98603/2
// But this could be better abstracted later to allow for Windows+Unix operation (TODO)

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct ProcessEvent {
    target_instance: Process,
}

// There's a chance that using PathBuf here might bite me in the ass?
// https://github.com/serde-rs/json/issues/550
#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_Process")]
#[serde(rename_all = "PascalCase")]
pub struct Process {
    pub process_id: u32,
    // #[serde(deserialize_with = "to_os_string")]
    pub name: PathBuf,
    pub executable_path: Option<PathBuf>,
}

impl Process {
    pub fn profile_matches(&self, profile: &AppOverride) -> bool {
        let needs_path = profile.process_path.is_absolute();

        match self.executable_path.as_ref() {
            // Expecting an absolute path
            None if needs_path => false,
            Some(path) if needs_path => *path == profile.process_path,
            // If not expecting an absolute path, then see if the process name matches
            _ => self.name == profile.process_path,
        }
    }
}

// Some(path) if needs_path => path.lossy_lowercase_cmp(&profile.process_path),

// trait LossyLowercaseCheck {
//     fn lossy_lowercase_cmp(&self, other: &PathBuf) -> bool;
// }

// impl LossyLowercaseCheck for PathBuf {
//     fn lossy_lowercase_cmp(&self, other: &PathBuf) -> bool {
//         match (self.to_str(), other.to_str()) {
//             // If we can get proper Unicode from the Path, do a case-insensitive match
//             (Some(l), Some(r)) => {
//                 debug!("Checking {l} vs {r}");
//                 l.eq_ignore_ascii_case(r)
//             }
//             // But if we can't, just check them directly
//             _ => {
//                 debug!("Failed to get str?");
//                 self == other
//             }
//         }
//     }
// }

// fn to_os_string<'de, D>(deserializer: D) -> Result<OsString, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     let buf: String = Deserialize::deserialize(deserializer)?;
//     Ok(OsString::from(buf))
// }

/// Task that updates a DashMap with the current running processes,
/// notifying the supplied EventLoopProxy when any change occurs.
pub fn process_event_loop(
    process_map: Arc<DashMap<u32, Process>>,
    map_updated: Sender<(usize, Option<LockFile>)>,
    event_proxy: AppEventProxy,
) -> AppResult<()> {
    let wmi_con = WMIConnection::new(COMLibrary::new()?)?;

    let initial_processes: Vec<Process> = wmi_con.query()?;
    for process in initial_processes {
        process_map.insert(process.process_id, process);
    }

    // let exe_path = std::env::current_exe()?;
    // let user_dir = get_user_dir().expect("Failed to get local user dir");
    let temp_dir = std::env::temp_dir();
    let lock_file_path = {
        // let exe_name = exe_path.file_stem().unwrap();
        // let temp_path = user_dir.join(exe_name);
        // temp_path.with_extension("lock")

        // Maybe hardcoded in env::temp_dir is better to *ensure* no duplicates are allowed.
        temp_dir.join("redefaulter.lock")
    };

    let lock_file = if lock_file_path.exists() {
        let contents = fs::read_to_string(&lock_file_path)?;
        let pid = contents
            .trim()
            .parse::<u32>()
            .map_err(|_| RedefaulterError::ParseLockFile)?;

        let already_running = process_map.iter().any(|p| p.process_id == pid);

        if already_running {
            None
        } else {
            Some(LockFile::build(&lock_file_path)?)
        }
    } else {
        Some(LockFile::build(&lock_file_path)?)
    };

    let instance_already_exists = lock_file.is_none();

    map_updated
        .send((process_map.len(), lock_file))
        .map_err(|_| RedefaulterError::ProcessUpdate)?;

    if instance_already_exists {
        return Ok(());
    }

    let query = concat!(
        // Get events
        "SELECT * FROM __InstanceOperationEvent ",
        // Every second
        "WITHIN 1 ",
        // Where the instance is a process
        "WHERE TargetInstance ISA ",
        "\"Win32_Process\" ",
        // And the event is creation or deletion
        "AND (__Class = \"__InstanceCreationEvent\" OR __Class = \"__InstanceDeletionEvent\")"
    );

    let enumerator = wmi_con.notification_native_wrapper(query)?;
    for item in enumerator {
        match item {
            Ok(wbem_class_obj) => {
                let class = wbem_class_obj.class()?;
                match class.as_str() {
                    "__InstanceCreationEvent" => {
                        let process = wbem_class_obj.into_desr::<ProcessEvent>()?.target_instance;
                        trace!("New process: {process:?}");
                        process_map.insert(process.process_id, process);
                    }
                    "__InstanceDeletionEvent" => {
                        let process = wbem_class_obj.into_desr::<ProcessEvent>()?.target_instance;
                        trace!("Closed process: {process:?}");
                        process_map.remove(&process.process_id);
                    }
                    _ => Err(WMIError::InvalidDeserializationVariantError(class))?,
                };
                event_proxy
                    .send_event(CustomEvent::ProcessesChanged)
                    .map_err(|_| RedefaulterError::EventLoopClosed)?;
            }
            Err(e) => Err(e)?,
        }
    }

    Ok(())
}

pub struct LockFile {
    // current_pid: u32,
    path: PathBuf,
}

impl LockFile {
    fn build(path: &Path) -> AppResult<Self> {
        let current_pid = std::process::id();
        let path = path.to_owned();
        fs::write(&path, current_pid.to_string())?;
        Ok(Self {
            // current_pid,
            path,
        })
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        fs::remove_file(&self.path).expect("Failed to remove lock file");
    }
}
