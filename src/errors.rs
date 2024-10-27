// use color_eyre::Result;

pub type AppResult<T> = Result<T, RedefaulterError>;

#[derive(Debug, thiserror::Error)]
pub enum RedefaulterError {
    #[error("Windows Error: {0}")]
    Windows(#[from] windows_result::Error),
    #[error("Windows Error: {0}")]
    WindowsCore(#[from] windows_core::Error),
    #[error("WMI Error: {0}")]
    Wmi(#[from] wmi::WMIError),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML Serialization Error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("TOML Deserialization Error: {0}")]
    TomlDe(#[from] toml::de::Error),
    // My errors
    #[error("Failed to get device info")]
    FailedToGetInfo,
    #[error("Failed to get active processes")]
    FailedToGetProcesses,
    #[error("Failed to get working directory")]
    WorkDir,
    #[error("Could not trigger process updated event")]
    ProcessUpdate,
    #[error("Event loop closed")]
    EventLoopClosed,
}
