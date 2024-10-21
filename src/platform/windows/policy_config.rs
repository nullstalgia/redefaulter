use std::ffi::c_void;

use windows::{
    core::*,
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::*,
        Graphics::{Gdi, Gdi::*},
        Media::Audio::{Endpoints::*, *},
        System::{
            Com::*, Console::*, LibraryLoader::GetModuleHandleW, Registry::*,
            SystemInformation::GetSystemDirectoryW, Threading::*,
        },
        UI::Shell::{PropertiesSystem::PROPERTYKEY, *},
    },
};

// Yoinked from https://github.com/DvdGiessen/microphone-mute-indicator/blob/e1b291efff0a5f89bc1242cbd14bff8ddd1a52a1/src/main.rs#L133

// Implementation of reversed engineered COM object for changing default audio endpoint
#[allow(non_upper_case_globals)]
pub const PolicyConfig: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

define_interface!(
    IPolicyConfig,
    IPolicyConfig_Vtbl,
    0xf8679f50_850a_41cf_9c72_430f290290c8
);
impl std::ops::Deref for IPolicyConfig {
    type Target = IUnknown;
    fn deref(&self) -> &Self::Target {
        unsafe { std::mem::transmute(self) }
    }
}
interface_hierarchy!(IPolicyConfig, IUnknown);
impl IPolicyConfig {
    #[allow(non_snake_case, clippy::missing_safety_doc)]
    pub unsafe fn SetDefaultEndpoint<P0>(&self, wszDeviceId: P0, role: ERole) -> Result<()>
    where
        P0: Param<PWSTR>,
    {
        (Interface::vtable(self).SetDefaultEndpoint)(
            Interface::as_raw(self),
            wszDeviceId.param().abi(),
            role,
        )
        .ok()
    }
}

#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct IPolicyConfig_Vtbl {
    pub base__: IUnknown_Vtbl,
    pub GetMixFormat: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        waveformatex: *mut c_void,
    ) -> HRESULT,
    pub GetDeviceFormat: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        param0: i32,
        waveformatex: *mut c_void,
    ) -> HRESULT,
    pub ResetDeviceFormat: unsafe extern "system" fn(this: c_void, pwstrid: PWSTR) -> HRESULT,
    pub SetDeviceFormat: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        waveformatex0: c_void,
        waveformatex1: *mut c_void,
    ) -> HRESULT,
    pub GetProcessingPeriod: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        param0: i32,
        param1: c_void,
        param1: *mut c_void,
    ) -> HRESULT,
    pub SetProcessingPeriod:
        unsafe extern "system" fn(this: c_void, pwstrid: PWSTR, param0: c_void) -> HRESULT,
    pub GetShareMode: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        devicesharemode: *mut c_void,
    ) -> HRESULT,
    pub SetShareMode: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        devicesharemode: *mut c_void,
    ) -> HRESULT,
    pub GetPropertyValue: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        key: c_void,
        propvariant: *mut c_void,
    ) -> HRESULT,
    pub SetPropertyValue: unsafe extern "system" fn(
        this: *mut c_void,
        pwstrid: PWSTR,
        key: c_void,
        propvariant: *mut c_void,
    ) -> HRESULT,
    pub SetDefaultEndpoint:
        unsafe extern "system" fn(this: *mut c_void, pwstrid: PWSTR, role: ERole) -> HRESULT,
    pub SetEndpointVisibility:
        unsafe extern "system" fn(this: *mut c_void, pwstrid: PWSTR, param0: i32) -> HRESULT,
}
