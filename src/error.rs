use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::fmt;

#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
    ParseError,
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotSupported,
    PayloadTooLarge,
    UnsupportedMediaType,
    InternalServerError,
}

impl ErrorCode {
    fn details(self) -> (&'static str, i32, StatusCode) {
        match self {
            Self::ParseError => ("PARSE_ERROR", -32700, StatusCode::BAD_REQUEST),
            Self::BadRequest => ("BAD_REQUEST", -32600, StatusCode::BAD_REQUEST),
            Self::Unauthorized => ("UNAUTHORIZED", -32001, StatusCode::UNAUTHORIZED),
            Self::Forbidden => ("FORBIDDEN", -32003, StatusCode::FORBIDDEN),
            Self::NotFound => ("NOT_FOUND", -32004, StatusCode::NOT_FOUND),
            Self::MethodNotSupported => (
                "METHOD_NOT_SUPPORTED",
                -32005,
                StatusCode::METHOD_NOT_ALLOWED,
            ),
            Self::PayloadTooLarge => ("PAYLOAD_TOO_LARGE", -32013, StatusCode::PAYLOAD_TOO_LARGE),
            Self::UnsupportedMediaType => (
                "UNSUPPORTED_MEDIA_TYPE",
                -32015,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
            Self::InternalServerError => (
                "INTERNAL_SERVER_ERROR",
                -32603,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        }
    }
}

#[derive(Debug)]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub(crate) fn envelope(self, path: Option<&str>) -> (StatusCode, Value) {
        let (name, code, status) = self.code.details();
        let mut data = json!({ "code": name, "httpStatus": status.as_u16() });
        if let Some(path) = path {
            data["path"] = Value::from(path);
        }
        (
            status,
            json!({ "error": { "message": self.message, "code": code, "data": data } }),
        )
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RpcError {}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        let (status, body) = self.envelope(None);
        (status, Json(body)).into_response()
    }
}
