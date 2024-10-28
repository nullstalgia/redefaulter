use crate::app::CustomEvent;
use crate::errors::{AppResult, RedefaulterError};
use crate::profiles::AppOverride;

use dashmap::DashMap;
use serde::{Deserialize, Deserializer};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::{collections::HashMap, sync::mpsc::Sender};
use tao::event_loop::EventLoopProxy;
use tracing::trace;
use wmi::*;

// Inspired by https://users.rust-lang.org/t/watch-for-windows-process-creation-in-rust/98603/2

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct ProcessEvent {
    target_instance: Process,
}

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
        // println!(
        //     "{:?} {:?} {:?}",
        //     self.executable_path, profile.process_path, needs_path
        // );
        match self.executable_path.as_ref() {
            // Expecting an absolute path
            None if needs_path => false,
            Some(path) if needs_path => *path == profile.process_path,
            // If not an absolute path, then see if the name matches
            None => self.name == profile.process_path,
            Some(_) => self.name == profile.process_path,
        }
    }
}

fn to_os_string<'de, D>(deserializer: D) -> Result<OsString, D::Error>
where
    D: Deserializer<'de>,
{
    let buf: String = Deserialize::deserialize(deserializer)?;
    Ok(OsString::from(buf))
}

/// Task that updates a DashMap with the current running processes,
/// notifying the supplied EventLoopProxy when any change occurs.
pub fn process_event_loop(
    process_map: Arc<DashMap<u32, Process>>,
    map_updated: Sender<usize>,
    event_proxy: EventLoopProxy<CustomEvent>,
) -> AppResult<()> {
    let wmi_con = WMIConnection::new(COMLibrary::new()?)?;

    let initial_processes: Vec<Process> = wmi_con.query()?;
    for process in initial_processes {
        process_map.insert(process.process_id, process);
    }

    map_updated
        .send(process_map.len())
        .map_err(|_| RedefaulterError::ProcessUpdate)?;

    let mut filters = HashMap::<String, FilterValue>::new();
    filters.insert("TargetInstance".to_owned(), FilterValue::is_a::<Process>()?);
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
