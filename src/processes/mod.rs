use crate::errors::{AppResult, RedefaulterError};
use crate::profiles::AppOverride;

use dashmap::DashMap;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::{collections::HashMap, sync::mpsc::Sender};
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
        // let test: OsString = String::from("A").into();
        let needs_path = profile.process_path.is_absolute();
        // println!(
        //     "{:?} {:?} {:?}",
        //     self.executable_path, profile.process_path, needs_path
        // );
        match self.executable_path.as_ref() {
            // Expected an absolute path but none existed
            None if needs_path => false,
            // If not an absolute path, then see if the name matches
            None => self.name == profile.process_path,
            // But if we have a path, see if they match
            Some(path) if needs_path => *path == profile.process_path,
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

#[derive(Debug, Clone)]
pub enum ProcessEventType {
    Created,
    Deleted,
    // Modified,
}

#[derive(Debug)]
/// Keeps track of open and closed processes internally, only sending events as
pub struct ProcessWatcher {
    pub processes: DashMap<u32, Process>,
}

impl ProcessWatcher {
    pub fn build() -> AppResult<Self> {
        // TODO check for another instance of the app, current_exe() in this task should make it easy
        let processes = DashMap::new();

        let wmi_con = WMIConnection::new(COMLibrary::new()?)?;

        let results: Vec<Process> = wmi_con.query()?;

        for process in results {
            processes.insert(process.process_id, process);
        }

        Ok(Self { processes })
    }
}

pub fn process_event_loop(
    process_map: Arc<DashMap<u32, Process>>,
    map_updated: Sender<usize>,
    // tx: Sender<(ProcessEventType, Process)>,
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
                use ProcessEventType::*;
                let class = wbem_class_obj.class()?;
                match class.as_str() {
                    "__InstanceCreationEvent" => {
                        let process = wbem_class_obj.into_desr::<ProcessEvent>()?.target_instance;
                        process_map.insert(process.process_id, process);
                    }
                    "__InstanceDeletionEvent" => {
                        let process = wbem_class_obj.into_desr::<ProcessEvent>()?.target_instance;
                        process_map.remove(&process.process_id);
                    }
                    // "__InstanceModificationEvent" => Modified,
                    _ => Err(WMIError::InvalidDeserializationVariantError(class))?,
                };
                map_updated
                    .send(process_map.len())
                    .map_err(|_| RedefaulterError::ProcessUpdate)?;
            }
            Err(e) => Err(e)?,
        }
    }
    // let iterator = enumerator.map(|item| match item {
    //     Ok(wbem_class_obj) => {
    //         use ProcessEventType::*;
    //         let class = wbem_class_obj.class()?;
    //         let event_type = match class.as_str() {
    //             "__InstanceCreationEvent" => Created,
    //             "__InstanceDeletionEvent" => Deleted,
    //             // "__InstanceModificationEvent" => Modified,
    //             _ => return Err(WMIError::InvalidDeserializationVariantError(class)),
    //         };
    //         Ok((event_type, wbem_class_obj.into_desr::<ProcessEvent>()?))
    //     }
    //     Err(e) => Err(e),
    // });

    // for result in iterator {
    //     let message = match result {
    //         Ok(message) => message,
    //         Err(e) => {
    //             eprintln!("Error with process message: {e}");
    //             break;
    //         }
    //     };
    //     if let Err(e) = tx.send((message.0, message.1.target_instance)) {
    //         eprintln!("Unable to send process event! Closing thread");
    //         break;
    //     };
    // }

    Ok(())
}
