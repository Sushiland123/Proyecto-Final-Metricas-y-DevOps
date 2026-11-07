//! Safe errors: adapter details and submitted values never cross the HTTP boundary.
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    MethodNotAllowed,
    Unauthenticated,
    Forbidden,
    NotFound,
    Duplicate,
    Unavailable,
    Internal,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: PublicError,
}
#[derive(Serialize, ToSchema)]
pub struct PublicError {
    pub code: &'static str,
    pub message: &'static str,
    pub request_id: String,
}

impl Error {
    pub fn details(self) -> (StatusCode, &'static str, &'static str) {
        match self {
            Self::InvalidInput => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_INPUT",
                "Invalid request input.",
            ),
            Self::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                "INVALID_INPUT",
                "Invalid request.",
            ),
            Self::Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHENTICATED",
                "Authentication required.",
            ),
            Self::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN", "Access denied."),
            Self::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND", "Resource not found."),
            Self::Duplicate => (StatusCode::CONFLICT, "DUPLICATE", "Event already exists."),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "DEPENDENCY_UNAVAILABLE",
                "Dependency unavailable.",
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "Internal server error.",
            ),
        }
    }
    pub fn response(self, request_id: String) -> Response {
        let (status, code, message) = self.details();
        (
            status,
            Json(ErrorEnvelope {
                error: PublicError {
                    code,
                    message,
                    request_id,
                },
            }),
        )
            .into_response()
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut response = self.response(String::new());
        response.extensions_mut().insert(self);
        response
    }
}
