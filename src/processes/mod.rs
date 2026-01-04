use crate::app::{AppEventProxy, CustomEvent};
use crate::errors::{AppResult, RedefaulterError};
use crate::profiles::AppOverride;

use dashmap::DashMap;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use tracing::*;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexA;
use windows_core::s;
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
    map_updated: Sender<usize>,
    event_proxy: AppEventProxy,
) -> AppResult<()> {
    let wmi_con = WMIConnection::new(COMLibrary::new()?)?;

    let initial_processes: Vec<Process> = wmi_con.query()?;
    for mut process in initial_processes {
        #[cfg(windows)]
        fix_system32_paths(&mut process);
        process_map.insert(process.process_id, process);
    }

    map_updated
        .send(process_map.len())
        .map_err(|_| RedefaulterError::ProcessUpdate)?;

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
                        let mut process =
                            wbem_class_obj.into_desr::<ProcessEvent>()?.target_instance;

                        #[cfg(windows)]
                        fix_system32_paths(&mut process);

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

#[cfg(windows)]
/// Paths leading from `Disk:/Windows/System32` can sometimes have the capitalization goofed up by WMI.
///
/// (For example, returning `system32` instead of the properly-capitalized `System32`, which can mess with expected behavior).
///
/// This function checks for and repairs such paths.
fn fix_system32_paths(input: &mut Process) {
    use std::ffi::OsString;
    use std::path::Prefix;

    let Some(executable_path) = input.executable_path.as_mut() else {
        return;
    };

    let mut comp = executable_path.components();

    use std::path::Component::*;
    // Getting the first four components, expecting: `[Disk, RootDir, "Windows", "System32"]`
    if let (Some(Prefix(prefix)), Some(RootDir), Some(Normal(path1)), Some(Normal(path2))) =
        (comp.next(), comp.next(), comp.next(), comp.next())
    {
        // Making sure we have *some* disk as the first component.
        // I dunno, someone may have System32 not in `C:`
        let Prefix::Disk(_) = prefix.kind() else {
            return;
        };
        // Make sure it's even a System32 path
        if !path1.to_ascii_lowercase().eq("windows") && !path2.to_ascii_lowercase().eq("system32") {
            return;
        }
        // Don't repair if it's already normal
        if path1.eq("Windows") && path2.eq("System32") {
            return;
        }
        let new_path: PathBuf = [
            Prefix(prefix),
            RootDir,
            Normal(&OsString::from("Windows")),
            Normal(&OsString::from("System32")),
        ]
        .into_iter()
        .chain(comp)
        .collect();
        *executable_path = new_path;
    }
}

pub struct LockFile {
    handle: HANDLE,
}

impl LockFile {
    pub fn build() -> AppResult<Self> {
        let app_mutex = unsafe { CreateMutexA(None, true, s!("Global\\RedefaulterLock")) }?;

        match unsafe { GetLastError() }.ok() {
            Ok(_) => (),
            Err(e) if e.code() == ERROR_ALREADY_EXISTS.to_hresult() => {
                return Err(RedefaulterError::AlreadyRunning);
            }
            Err(e) => return Err(e.into()),
        }

        Ok(Self { handle: app_mutex })
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        if let Err(e) = unsafe { CloseHandle(self.handle) } {
            error!("Failed to drop app mutex! {e}");
        }
    }
}
