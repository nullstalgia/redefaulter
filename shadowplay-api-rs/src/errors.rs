pub type ApiResult<T> = core::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ShadowPlay API Error: {0:?}")]
    ApiResponse(Option<crate::structs::ErrorResponse>),
    #[error("JSON Error: ")]
    Json(#[from] serde_json::Error),
    #[error("Reqwest Error: ")]
    Reqwest(#[from] reqwest::Error),
    #[error("Windows Error: ")]
    Windows(#[from] windows_result::Error),
    #[error("Bad token conversion to HTTP Header value")]
    HeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error("URL Forming Error: {0}")]
    UrlForm(String),
    #[error("Invalid secret file handle")]
    InvalidHandle,
    #[error("Could not map view of file")]
    MemMap,
    #[error("ShadowPlay Security token is invalid")]
    InvalidToken,
    #[error("Microphone by GUID \"{0}\" not found")]
    MicNotFound(String),
}
