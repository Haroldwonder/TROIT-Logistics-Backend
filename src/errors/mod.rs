use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AppError {
    #[error("Bad Request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not Found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Validation Error: {0}")]
    ValidationError(String),

    #[error("Blockchain Error: {0}")]
    BlockchainError(String),

    #[error("Internal Server Error")]
    InternalServerError(String),
}

#[derive(Serialize)]
#[allow(dead_code)]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::ValidationError(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg),
            AppError::BlockchainError(msg) => {
                tracing::error!("Blockchain Error: {}", msg);
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Blockchain Service Failure: {}", msg),
                )
            }
            AppError::InternalServerError(internal_msg) => {
                // Log internal details safely on server side
                tracing::error!("Internal Server Error: {}", internal_msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An unexpected error occurred on the server. Please try again later."
                        .to_string(),
                )
            }
        };

        let body = Json(ErrorResponse {
            success: false,
            message,
        });

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => {
                AppError::NotFound("Requested resource not found".to_string())
            }
            sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
                AppError::Conflict("A record with these details already exists".to_string())
            }
            other => AppError::InternalServerError(other.to_string()),
        }
    }
}
