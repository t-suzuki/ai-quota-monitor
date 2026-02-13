use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Unsupported service")]
    UnsupportedService,
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Store error: {0}")]
    Store(String),
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("Window error: {0}")]
    Window(String),
    #[error("API error: {0}")]
    Api(#[from] crate::api_client::ApiError),
    #[error("{0}")]
    Message(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<String> for AppError {
    fn from(value: String) -> Self {
        AppError::Message(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        AppError::Message(value.to_string())
    }
}
