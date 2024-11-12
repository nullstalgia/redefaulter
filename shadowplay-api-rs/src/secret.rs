use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use reqwest::header::HeaderValue;
use serde_derive::Deserialize;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ},
    },
};

use crate::errors::{ApiResult, Error};

pub const SECRET_HEADER: &str = "X_LOCAL_SECURITY_COOKIE";
const SECRET_FILE: &str = "{8BA1E16C-FC54-4595-9782-E370A5FBE8DA}";

#[derive(Debug, Deserialize)]
pub struct SecretContents {
    pub port: u32,
    #[serde(rename = "secret")]
    pub token: String,
}

impl SecretContents {
    pub fn load() -> ApiResult<Self> {
        // Convert the mapping name to a null-terminated wide string
        let wide_mapping_name: Vec<u16> = OsStr::new(SECRET_FILE)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            // Open the named file mapping object
            let h_map_file: HANDLE =
                OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(wide_mapping_name.as_ptr()))?;

            if h_map_file.is_invalid() {
                return Err(Error::InvalidHandle);
            }

            // Map a view of the file into the address space
            let p_buf = MapViewOfFile(
                h_map_file,
                FILE_MAP_READ,
                0,
                0,
                0, // Map the entire file
            );

            if p_buf.Value.is_null() {
                CloseHandle(h_map_file)?;
                return Err(Error::MemMap);
            }

            // Reading data (assuming it's a C-style string)
            let c_str = std::ffi::CStr::from_ptr(p_buf.Value as *const i8);

            let secret: SecretContents = serde_json::from_slice(c_str.to_bytes())?;

            // When done, unmap and close the handle
            UnmapViewOfFile(p_buf)?;
            CloseHandle(h_map_file)?;

            Ok(secret)
        }
    }
    pub fn as_header_value(&self) -> ApiResult<HeaderValue> {
        HeaderValue::from_str(&self.token).map_err(Error::HeaderValue)
    }
}
