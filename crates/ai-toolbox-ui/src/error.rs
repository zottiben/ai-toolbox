//! One error shape for the whole API.
//!
//! The refusals the core produces carry the facts a person needs - which path, which
//! preset, what was wrong with it. Flattening them into "500 internal error" would throw
//! away exactly the information the board exists to show, so each keeps its own status
//! and a stable `code` the client can switch on without parsing a message.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use ai_toolbox_core::Error as CoreError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("missing or wrong token")]
    Unauthorized,

    #[error(transparent)]
    Internal(CoreError),
}

impl Error {
    pub fn not_found(message: impl Into<String>) -> Error {
        Error::NotFound(message.into())
    }

    pub fn bad_request(message: impl Into<String>) -> Error {
        Error::BadRequest(message.into())
    }

    fn status(&self) -> StatusCode {
        match self {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            Error::Unauthorized => StatusCode::UNAUTHORIZED,
            Error::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "not_found",
            Error::BadRequest(_) => "bad_request",
            Error::Unauthorized => "unauthorized",
            Error::Internal(_) => "internal",
        }
    }
}

/// A catalogue complaint is the user naming something that is not there - a bad request,
/// not a server fault. Everything else genuinely went wrong underneath.
impl From<CoreError> for Error {
    fn from(err: CoreError) -> Error {
        match err {
            CoreError::Catalogue(message) => Error::BadRequest(message),
            other => Error::Internal(other),
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct Body {
    error: String,
    code: &'static str,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let body = Body {
            error: self.to_string(),
            code: self.code(),
        };
        (self.status(), Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, Error>;
