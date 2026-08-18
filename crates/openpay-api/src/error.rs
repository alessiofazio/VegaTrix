use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use openpay_application::{ApplicationError, RepositoryError};
use openpay_domain::DomainError;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProblemDetails {
    pub r#type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl ProblemDetails {
    pub fn new(status: StatusCode, kind: &str, title: &str, detail: impl Into<String>) -> Self {
        Self {
            r#type: format!("https://openpay.local/problems/{kind}"),
            title: title.into(),
            status: status.as_u16(),
            detail: detail.into(),
            instance: None,
        }
    }
}

pub struct ApiError {
    pub status: StatusCode,
    pub problem: ProblemDetails,
}

impl ApiError {
    pub fn new(status: StatusCode, kind: &str, title: &str, detail: impl Into<String>) -> Self {
        Self {
            status,
            problem: ProblemDetails::new(status, kind, title, detail),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.problem)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            "application/problem+json".parse().unwrap(),
        );
        response
    }
}

impl From<ApplicationError> for ApiError {
    fn from(err: ApplicationError) -> Self {
        match err {
            ApplicationError::Domain(DomainError::IllegalTransition { from, to }) => Self::new(
                StatusCode::CONFLICT,
                "illegal-transition",
                "Illegal payment transition",
                format!("{from} → {to}"),
            ),
            ApplicationError::Domain(other) => Self::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                other.to_string(),
            ),
            ApplicationError::Repository(RepositoryError::NotFound) => Self::new(
                StatusCode::NOT_FOUND,
                "not-found",
                "Not found",
                "resource not found",
            ),
            ApplicationError::Repository(RepositoryError::IdempotencyMismatch) => Self::new(
                StatusCode::CONFLICT,
                "idempotency",
                "Idempotency key reused with different payload",
                "fingerprint mismatch",
            ),
            ApplicationError::Repository(RepositoryError::VersionConflict) => Self::new(
                StatusCode::CONFLICT,
                "version-conflict",
                "Optimistic lock conflict",
                "retry the request",
            ),
            ApplicationError::Repository(RepositoryError::Conflict(msg)) => {
                Self::new(StatusCode::CONFLICT, "conflict", "Conflict", msg)
            }
            ApplicationError::Forbidden | ApplicationError::Replay => Self::new(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Forbidden",
                "token rejected",
            ),
            ApplicationError::Expired => Self::new(
                StatusCode::GONE,
                "expired",
                "Expired",
                "token or payment expired",
            ),
            ApplicationError::Routing(msg) | ApplicationError::Connector(msg) => Self::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "connector",
                "Connector error",
                msg,
            ),
            ApplicationError::Repository(RepositoryError::Infra(msg)) => {
                tracing::error!(error = %msg, "infrastructure error");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "Internal error",
                    "an internal error occurred",
                )
            }
        }
    }
}
