use serde::Serialize;

use serde_json::json;
use thiserror::Error;
use tracing::error;
use yam_http::server::{IntoResponse, Response, StatusCode};

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("email already exists")]
    EmailAlreadyExists,
    #[error("internal server error")]
    InternalServerError,
    #[error("invalid token")]
    InvalidToken,
    #[error("resource not found")]
    NotFound,
    #[error("Unauthorized")]
    UnAuthorized,
    #[error("Database error")]
    Database(#[from] diesel::result::Error),

    #[error("password error")]
    Password(#[from] bcrypt::BcryptError),
    #[error("failed to parse uuid")]
    Uuid(#[from] uuid::Error),

    #[error("http error: {0}")]
    Http(#[from] yam_http::server::HttpError),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match &self {
            AppError::InvalidCredentials | AppError::Uuid(_) | AppError::Http(_) => {
                StatusCode::StatusBadRequest
            }
            AppError::EmailAlreadyExists => StatusCode::StatusConflict,
            AppError::UnAuthorized => StatusCode::StatusUnauthorized,
            AppError::InvalidToken => StatusCode::StatusBadRequest,
            AppError::NotFound => StatusCode::StatusFound,
            AppError::Database(_) | AppError::Password(_) => StatusCode::StatusInternalServerError,
            AppError::InternalServerError => StatusCode::StatusInternalServerError,
        }
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        error!("{self:?}");
        let status = self.status_code();
        let body = json!({
            "message": self.to_string(),
            "success": false,
        });
        Response::new().status(status).json(&body)
    }
}
