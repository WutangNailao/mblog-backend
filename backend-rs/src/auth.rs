use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use futures_util::future::LocalBoxFuture;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::config::AppConfig;
use crate::entity::{dev_token, user};
use crate::error::AppError;

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: i32,
    pub role: Option<String>,
    #[allow(dead_code)]
    pub device: String,
}

#[derive(Clone, Debug)]
pub struct OptionalAuthUser(pub Option<AuthUser>);

impl FromRequest for AuthUser {
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let db = match req.app_data::<web::Data<DatabaseConnection>>() {
            Some(db) => db.clone(),
            None => {
                return Box::pin(async { Err(AppError::system_exception().into()) });
            }
        };
        let config = match req.app_data::<web::Data<AppConfig>>() {
            Some(cfg) => cfg.clone(),
            None => {
                return Box::pin(async { Err(AppError::system_exception().into()) });
            }
        };
        let token = extract_token(req, &config);

        Box::pin(async move {
            let token = token.ok_or_else(|| AppError::need_login())?;
            let auth = authenticate_token(&db, &config, &token).await?;
            Ok(auth)
        })
    }
}

impl FromRequest for OptionalAuthUser {
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let db = match req.app_data::<web::Data<DatabaseConnection>>() {
            Some(db) => db.clone(),
            None => {
                return Box::pin(async { Ok(OptionalAuthUser(None)) });
            }
        };
        let config = match req.app_data::<web::Data<AppConfig>>() {
            Some(cfg) => cfg.clone(),
            None => {
                return Box::pin(async { Ok(OptionalAuthUser(None)) });
            }
        };
        let token = extract_token(req, &config);

        Box::pin(async move {
            if let Some(token) = token {
                let auth = authenticate_token(&db, &config, &token).await.ok();
                return Ok(OptionalAuthUser(auth));
            }
            Ok(OptionalAuthUser(None))
        })
    }
}

fn extract_token(req: &HttpRequest, config: &AppConfig) -> Option<String> {
    let header = config.token_header.as_str();
    req.headers()
        .get(header)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

async fn authenticate_token(
    db: &DatabaseConnection,
    config: &AppConfig,
    token: &str,
) -> Result<AuthUser, AppError> {
    let decoded = decode_jwt(config, token)?;
    let user_id = extract_user_id(&decoded).ok_or_else(AppError::need_login)?;
    let role = user::Entity::find_by_id(user_id)
        .one(db)
        .await
        .map_err(|_| AppError::system_exception())?
        .and_then(|u| u.role);

    let device = extract_device(&decoded).unwrap_or_else(|| "WEB".to_string());
    if device == "API" {
        let exists = dev_token::Entity::find()
            .filter(dev_token::Column::Token.eq(token))
            .filter(dev_token::Column::UserId.eq(user_id))
            .one(db)
            .await
            .map_err(|_| AppError::system_exception())?
            .is_some();
        if !exists {
            return Err(AppError::api_token_invalid());
        }
    }

    Ok(AuthUser { user_id, role, device })
}

fn decode_jwt(config: &AppConfig, token: &str) -> Result<serde_json::Value, AppError> {
    let key = DecodingKey::from_secret(config.jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = false;
    decode::<serde_json::Value>(token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|_| AppError::need_login())
}

fn extract_user_id(claims: &serde_json::Value) -> Option<i32> {
    for key in ["loginId", "userId", "id", "sub", "login_id"] {
        if let Some(value) = claims.get(key) {
            if let Some(id) = value.as_i64() {
                return Some(id as i32);
            }
            if let Some(s) = value.as_str() {
                if let Ok(id) = s.parse::<i32>() {
                    return Some(id);
                }
            }
        }
    }
    None
}

fn extract_device(claims: &serde_json::Value) -> Option<String> {
    for key in ["device", "loginType", "login_type", "deviceType"] {
        if let Some(value) = claims.get(key) {
            if let Some(s) = value.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}
