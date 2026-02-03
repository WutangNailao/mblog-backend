use actix_web::{web, HttpResponse};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::entity::dev_token;
use crate::error::AppError;
use crate::response::ResponseDto;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("").route(web::get().to(get_token)))
        .service(web::resource("/").route(web::get().to(get_token)))
        .service(web::resource("/reset").route(web::post().to(reset_token)))
        .service(web::resource("/enable").route(web::post().to(enable_token)))
        .service(web::resource("/disable").route(web::post().to(disable_token)));
}

#[derive(Serialize)]
struct TokenDto {
    id: i32,
    name: String,
    token: String,
}

#[derive(Serialize)]
struct EmptyResponse {}

async fn get_token(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let token = dev_token::Entity::find()
        .filter(dev_token::Column::Name.eq("default"))
        .filter(dev_token::Column::UserId.eq(auth.user_id))
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    let dto = token.map(|t| TokenDto {
        id: t.id,
        name: t.name,
        token: t.token,
    });

    Ok(HttpResponse::Ok().json(ResponseDto::success(dto)))
}

#[derive(serde::Deserialize)]
struct ResetQuery {
    id: i32,
}

async fn reset_token(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    config: web::Data<AppConfig>,
    query: web::Query<ResetQuery>,
) -> Result<HttpResponse, AppError> {
    let token = dev_token::Entity::find()
        .filter(dev_token::Column::Name.eq("default"))
        .filter(dev_token::Column::UserId.eq(auth.user_id))
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    if token.is_none() {
        return Err(AppError::fail("token不存在"));
    }

    let new_token = generate_token(&config, auth.user_id, "API")?;
    let active = dev_token::ActiveModel {
        id: Set(query.id),
        token: Set(new_token),
        ..Default::default()
    };

    dev_token::Entity::update(active)
        .exec(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    Ok(HttpResponse::Ok().json(ResponseDto::<EmptyResponse>::success(None)))
}

async fn enable_token(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    config: web::Data<AppConfig>,
) -> Result<HttpResponse, AppError> {
    let token = dev_token::Entity::find()
        .filter(dev_token::Column::Name.eq("default"))
        .filter(dev_token::Column::UserId.eq(auth.user_id))
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    if token.is_none() {
        let token = generate_token(&config, auth.user_id, "API")?;
        let active = dev_token::ActiveModel {
            name: Set("default".to_string()),
            token: Set(token),
            user_id: Set(auth.user_id),
            ..Default::default()
        };
        active
            .insert(db.get_ref())
            .await
            .map_err(|_| AppError::system_exception())?;
    }

    Ok(HttpResponse::Ok().json(ResponseDto::<EmptyResponse>::success(None)))
}

async fn disable_token(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    dev_token::Entity::delete_many()
        .filter(dev_token::Column::Name.eq("default"))
        .filter(dev_token::Column::UserId.eq(auth.user_id))
        .exec(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    Ok(HttpResponse::Ok().json(ResponseDto::<EmptyResponse>::success(None)))
}

#[derive(serde::Serialize)]
struct TokenClaims {
    #[serde(rename = "loginId")]
    login_id: i32,
    device: String,
    exp: usize,
}

fn generate_token(config: &AppConfig, user_id: i32, device: &str) -> Result<String, AppError> {
    let exp = (Utc::now() + Duration::days(365 * 100)).timestamp() as usize;
    let claims = TokenClaims {
        login_id: user_id,
        device: device.to_string(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|_| AppError::system_exception())
}
