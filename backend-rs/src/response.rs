use actix_web::{error::JsonPayloadError, HttpRequest, HttpResponse};
use serde::Serialize;

use crate::error::AppError;

#[derive(Serialize)]
pub struct ResponseDto<T: Serialize> {
    pub data: Option<T>,
    pub code: i32,
    pub msg: String,
}

impl<T: Serialize> ResponseDto<T> {
    pub fn success(data: Option<T>) -> Self {
        Self {
            data,
            code: 0,
            msg: "".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn fail(code: i32, msg: impl Into<String>) -> Self {
        Self {
            data: None,
            code,
            msg: msg.into(),
        }
    }
}

pub fn json_error_handler(err: JsonPayloadError, _req: &HttpRequest) -> actix_web::Error {
    let app_err = match err {
        JsonPayloadError::ContentType => AppError::param_error("请求参数不合法"),
        JsonPayloadError::Deserialize(_) => AppError::param_error("请求参数不合法"),
        _ => AppError::param_error("请求参数不合法"),
    };
    app_err.into()
}

pub fn response_from_error(err: &AppError) -> HttpResponse {
    HttpResponse::Ok().json(ResponseDto::<()> {
        data: None,
        code: err.code(),
        msg: err.msg().to_string(),
    })
}
