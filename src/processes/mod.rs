use crate::errors::AppResult;

use serde::Deserialize;
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
    process_id: u32,
    name: String,
    executable_path: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ProcessEventType {
    Created,
    Modified,
    Deleted,
}

pub fn process_event_loop(tx: Sender<(ProcessEventType, Process)>) -> AppResult<()> {
    let mut filters = HashMap::<String, FilterValue>::new();

    filters.insert("TargetInstance".to_owned(), FilterValue::is_a::<Process>()?);
    let wmi_con = WMIConnection::new(COMLibrary::new()?)?;

    let query = "SELECT * FROM __InstanceOperationEvent WITHIN 1 WHERE TargetInstance ISA \"Win32_Process\" AND (__Class = \"__InstanceCreationEvent\" OR __Class = \"__InstanceDeletionEvent\")";

    let enumerator = wmi_con.notification_native_wrapper(query)?;
    let iterator = enumerator.map(|item| match item {
        Ok(wbem_class_obj) => {
            let class = wbem_class_obj.class()?;
            use ProcessEventType::*;
            let event_type = match class.as_str() {
                "__InstanceCreationEvent" => Created,
                "__InstanceDeletionEvent" => Deleted,
                "__InstanceModificationEvent" => Modified,
                _ => panic!(),
            };
            Ok((event_type, wbem_class_obj.into_desr::<ProcessEvent>()))
        }
        Err(e) => Err(e),
    });

    for result in iterator {
        let message = result?;
        if message.1.is_err() {
            // TODO log
            eprintln!("Error deserializing Process!");
            continue;
        }
        if let Err(e) = tx.send((message.0, message.1.unwrap().target_instance)) {
            eprintln!("Unable to send process event! Closing thread");
            break;
            // return Err(e);
        };
        // let event_type = result.as_ref().unwrap().0.clone();
        // let process = result.unwrap().1.unwrap().target_instance;
        // println!("{event_type:?} process!");
        // println!("PID:        {}", process.process_id);
        // println!("Name:       {}", process.name);
        // println!("Executable: {:?}", process.executable_path);
    }

    Ok(())
}
