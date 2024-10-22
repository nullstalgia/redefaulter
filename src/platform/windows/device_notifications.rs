// "Inspired" by https://github.com/fmsyt/output-switcher/blob/1528d44747793ab4e42d23761e021976a3113d98/src-tauri/src/ipc/audio/notifier.rs#L25

use color_eyre::Result;
use std::fmt::Debug;
use std::sync::mpsc::Sender;
use wasapi::{Direction, Role};
use windows::{
    core::{implement, PCWSTR},
    Win32::{
        Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_DATA, WIN32_ERROR},
        Media::Audio::{
            EDataFlow, ERole,
            Endpoints::{
                // IAudioEndpointVolume,
                IAudioEndpointVolumeCallback,
                IAudioEndpointVolumeCallback_Impl,
            },
            IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
        },
        UI::Shell::PropertiesSystem::PROPERTYKEY,
    },
};

use crate::errors::AppResult;

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
        state: u32,
    },
    PropertyValueChanged {
        id: String,
        key: String,
    },
    VolumeChanged {
        id: String,
        volume: f32,
        muted: bool,
    },
    // SessionVolumeChanged {
    //     id: String,
    //     volume: f32,
    //     muted: bool,
    // },
}

#[implement(IMMNotificationClient)]
#[allow(non_camel_case_types)]
struct AppEventHandlerClient(Sender<WindowsAudioNotification>);

impl IMMNotificationClient_Impl for AppEventHandlerClient {
    fn OnDeviceStateChanged(
        &self,
        pwstrdeviceid: &PCWSTR,
        dwnewstate: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        unsafe {
            self.0
                .send(WindowsAudioNotification::DeviceStateChanged {
                    id: pwstrdeviceid
                        .to_string()
                        .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?,
                    state: dwnewstate.0,
                })
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }

    fn OnDeviceAdded(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        unsafe {
            self.0
                .send(WindowsAudioNotification::DeviceAdded {
                    id: pwstrdeviceid
                        .to_string()
                        .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?,
                })
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }

    fn OnDeviceRemoved(&self, pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        unsafe {
            self.0
                .send(WindowsAudioNotification::DeviceRemoved {
                    id: pwstrdeviceid
                        .to_string()
                        .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?,
                })
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
                .send(WindowsAudioNotification::DefaultDeviceChanged { id, flow, role })
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        pwstrdeviceid: &PCWSTR,
        key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        unsafe {
            self.0
                .send(WindowsAudioNotification::PropertyValueChanged {
                    id: pwstrdeviceid
                        .to_string()
                        .map_err(|e| to_win_error(e, ERROR_INVALID_DATA))?,
                    key: format!("{:?}", key.fmtid),
                })
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }
}

pub(crate) struct NotificationCallbacks {
    notification_client: IMMNotificationClient,
    // endpoint_volume_callback: IAudioEndpointVolumeCallback,
}

impl NotificationCallbacks {
    pub(crate) fn new(tx: &Sender<WindowsAudioNotification>) -> Self {
        let notification_client = AppEventHandlerClient(tx.clone()).into();
        // let endpoint_volume_callback = AudioEndpointVolumeCallback(tx.clone()).into();

        Self {
            notification_client,
            // endpoint_volume_callback,
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

    // pub(crate) fn register_to_volume(&self, volume: &IAudioEndpointVolume) -> Result<()> {
    //     unsafe {
    //         volume.RegisterControlChangeNotify(&self.endpoint_volume_callback)?;
    //     }

    //     Ok(())
    // }

    // pub(crate) fn unregister_to_volume(&self, volume: &IAudioEndpointVolume) -> Result<()> {
    //     unsafe {
    //         volume.UnregisterControlChangeNotify(&self.endpoint_volume_callback)?;
    //     }

    //     Ok(())
    // }
}

#[implement(IAudioEndpointVolumeCallback)]
#[allow(non_camel_case_types)]
struct AudioEndpointVolumeCallback(Sender<WindowsAudioNotification>);

impl IAudioEndpointVolumeCallback_Impl for AudioEndpointVolumeCallback {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        unsafe {
            if data == std::ptr::null_mut() {
                return Err(to_win_error("data is null", ERROR_INVALID_DATA));
            }

            self.0
                .send(WindowsAudioNotification::VolumeChanged {
                    // .send(Notification::VolumeChanged {
                    id: format!("{:?}", (*data).guidEventContext),
                    volume: (*data).fMasterVolume,
                    muted: (*data).bMuted.as_bool(),
                })
                .map_err(|e| to_win_error(e, ERROR_ACCESS_DENIED))?;
        }

        Ok(())
    }
}
