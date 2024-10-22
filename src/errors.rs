// use color_eyre::Result;

pub type AppResult<T> = Result<T, RedefaulterError>;

#[derive(Debug, thiserror::Error)]
pub enum RedefaulterError {
    #[error("Windows Error: {0}")]
    Windows(#[from] windows_result::Error),
    #[error("Windows Error: {0}")]
    WindowsCore(#[from] windows_core::Error),
    #[error("Failed to get device info")]
    FailedToGetInfo,
}
