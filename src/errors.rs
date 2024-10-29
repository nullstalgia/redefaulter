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
    #[error("Wasapi Error: {0}")]
    Wasapi(#[from] wasapi::WasapiError),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML Serialization Error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("TOML Deserialization Error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("Tray Error: {0}")]
    Tray(#[from] tray_icon::Error),
    #[error("Tray Menu Error: {0}")]
    TrayMenu(#[from] tray_icon::menu::Error),
    #[error("Icon Error: {0}")]
    TrayIcon(#[from] tray_icon::BadIcon),
    // My errors
    #[error("Failed to get active processes")]
    FailedToGetProcesses,
    #[error("Failed to get working directory")]
    WorkDir,
    #[error("Could not trigger process updated event")]
    ProcessUpdate,
    #[error("Event loop closed")]
    EventLoopClosed,
    #[error("An instance of the application is already open")]
    AlreadyExists,
}
