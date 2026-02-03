use actix_web::{http::StatusCode, ResponseError};
use thiserror::Error;

use crate::response::response_from_error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{msg}")]
    Biz { code: i32, msg: String },
}

impl AppError {
    pub fn param_error(msg: impl Into<String>) -> Self {
        Self::Biz { code: 1, msg: msg.into() }
    }

    pub fn fail(msg: impl Into<String>) -> Self {
        Self::Biz { code: 2, msg: msg.into() }
    }

    pub fn need_login() -> Self {
        Self::Biz { code: 3, msg: "please login first".to_string() }
    }

    pub fn api_token_invalid() -> Self {
        Self::Biz { code: 3, msg: "api token已失效".to_string() }
    }

    #[allow(dead_code)]
    pub fn file_size_limit(msg: impl Into<String>) -> Self {
        Self::Biz { code: 4, msg: msg.into() }
    }

    pub fn system_exception() -> Self {
        Self::Biz { code: 99, msg: "system_exception".to_string() }
    }

    pub fn code(&self) -> i32 {
        match self {
            Self::Biz { code, .. } => *code,
        }
    }

    pub fn msg(&self) -> &str {
        match self {
            Self::Biz { msg, .. } => msg,
        }
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        StatusCode::OK
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        response_from_error(self)
    }
}
