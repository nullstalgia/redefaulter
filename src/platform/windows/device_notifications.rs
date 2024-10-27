// "Inspired" by https://github.com/fmsyt/output-switcher/blob/1528d44747793ab4e42d23761e021976a3113d98/src-tauri/src/ipc/audio/notifier.rs#L25

use std::fmt::Debug;
use std::sync::mpsc::Sender;
use tao::event_loop::EventLoopProxy;
use wasapi::{Direction, Role};
use windows::{
    core::{implement, PCWSTR},
    Win32::{
        Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_DATA, WIN32_ERROR},
        Media::Audio::{
            EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
            IMMNotificationClient_Impl, DEVICE_STATE,
        },
        UI::Shell::PropertiesSystem::PROPERTYKEY,
    },
};

use crate::{app::CustomEvent, errors::AppResult};

fn to_win_error<E: Debug>(e: E, code: WIN32_ERROR) -> windows::core::Error {
    windows::core::Error::new::<String>(code.to_hresult(), format!("{:?}", e).into())
}

#[derive(Debug, Clone)]
// #[allow(non_camel_case_types)]
pub enum WindowsAudioNotification {
    DefaultDeviceChanged {
        id: String,
        flow: Direction,
        role: Role,
    },
    DeviceAdded {
        id: String,
    },
    DeviceRemoved {
        id: String,
    },
    DeviceStateChanged {
        id: String,
        state: DEVICE_STATE,
    },
}

#[implement(IMMNotificationClient)]
#[allow(non_camel_case_types)]
// Bit of a circular dependency, not a fan.
struct AppEventHandlerClient(EventLoopProxy<CustomEvent>);

impl IMMNotificationClient_Impl for AppEventHandlerClient {
    fn OnDeviceStateChanged(
        &self,
        pwstrdeviceid: &PCWSTR,
        dwnewstate: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        unsafe {
            self.0
                .send_event(CustomEvent::AudioEndpointNotification(
                    WindowsAudioNotification::DeviceStateChanged {
                        id: pwstrdeviceid
                            .to_string()
                            .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?,
                        state: dwnewstate,
                    },
                ))
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
            // self.0
            //     .send()
            //     .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }

    fn OnDeviceAdded(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        unsafe {
            self.0
                .send_event(CustomEvent::AudioEndpointNotification(
                    WindowsAudioNotification::DeviceAdded {
                        id: pwstrdeviceid
                            .to_string()
                            .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?,
                    },
                ))
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }

    fn OnDeviceRemoved(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        unsafe {
            self.0
                .send_event(CustomEvent::AudioEndpointNotification(
                    WindowsAudioNotification::DeviceRemoved {
                        id: pwstrdeviceid
                            .to_string()
                            .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?,
                    },
                ))
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        pwstrdefaultdeviceid: &PCWSTR,
    ) -> windows::core::Result<()> {
        unsafe {
            let id = pwstrdefaultdeviceid
                .to_string()
                .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?;
            let flow =
                Direction::try_from(flow).map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?;
            let role = Role::try_from(role).map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?;
            self.0
                .send_event(CustomEvent::AudioEndpointNotification(
                    WindowsAudioNotification::DefaultDeviceChanged { id, flow, role },
                ))
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _pwstrdeviceid: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

pub(crate) struct NotificationCallbacks {
    notification_client: IMMNotificationClient,
}

impl NotificationCallbacks {
    pub(crate) fn new(tx: EventLoopProxy<CustomEvent>) -> Self {
        let notification_client = AppEventHandlerClient(tx).into();

        Self {
            notification_client,
        }
    }

    pub(crate) fn register_to_enumerator(
        &self,
        device_enumerator: &IMMDeviceEnumerator,
    ) -> AppResult<()> {
        unsafe {
            device_enumerator.RegisterEndpointNotificationCallback(&self.notification_client)?;
        }

        Ok(())
    }

    pub(crate) fn unregister_to_enumerator(
        &self,
        device_enumerator: &IMMDeviceEnumerator,
    ) -> AppResult<()> {
        unsafe {
            device_enumerator.UnregisterEndpointNotificationCallback(&self.notification_client)?;
        }

        Ok(())
    }
}
