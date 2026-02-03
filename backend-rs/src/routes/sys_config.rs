use actix_web::{web, HttpResponse};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::entity::sys_config;
use crate::error::AppError;
use crate::response::ResponseDto;
use crate::sys_config as sys_config_store;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/save").route(web::post().to(save)))
        .service(web::resource("/get").route(web::get().to(get_all)))
        .service(web::resource("/").route(web::get().to(get_front_config)));
}

#[derive(Deserialize)]
struct SaveSysConfigRequest {
    items: Option<Vec<SysConfigDto>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SysConfigDto {
    key: String,
    value: Option<String>,
}

pub async fn init_defaults(db: &DatabaseConnection) {
    let token = sys_config_store::get_string(db, WEB_HOOK_TOKEN)
        .await
        .ok()
        .flatten();
    if token.is_none() || token.as_deref() == Some("") {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let encoded = STANDARD.encode(bytes);
        let _ = upsert_config(db, WEB_HOOK_TOKEN, Some(encoded)).await;
    }
}

async fn save(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    payload: web::Json<SaveSysConfigRequest>,
) -> Result<HttpResponse, AppError> {
    require_admin(&auth)?;

    let items = payload.items.clone().ok_or_else(|| AppError::param_error("items must not be null"))?;

    for item in items {
        upsert_config(db.get_ref(), &item.key, item.value).await?;
    }

    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

async fn get_all(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&auth)?;
    let list = sys_config::Entity::find()
        .all(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;
    let dto = list.into_iter().map(to_dto).collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(dto))))
}

async fn get_front_config(
    db: web::Data<DatabaseConnection>,
) -> Result<HttpResponse, AppError> {
    let keys = vec![
        OPEN_REGISTER,
        WEBSITE_TITLE,
        OPEN_COMMENT,
        OPEN_LIKE,
        MEMO_MAX_LENGTH,
        INDEX_WIDTH,
        USER_MODEL,
        CUSTOM_CSS,
        CUSTOM_JAVASCRIPT,
        THUMBNAIL_SIZE,
        ANONYMOUS_COMMENT,
        COMMENT_APPROVED,
    ];

    let list: Vec<sys_config::Model> = sys_config::Entity::find()
        .filter(sys_config::Column::Key.is_in(keys.iter().map(|s| s.to_string())))
        .all(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    let dto = list.into_iter().map(to_dto).collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(dto))))
}

fn to_dto(model: sys_config::Model) -> SysConfigDto {
    let value = match model.value {
        Some(v) if !v.is_empty() => Some(v),
        _ => model.default_value,
    };
    SysConfigDto {
        key: model.key,
        value,
    }
}

async fn upsert_config(
    db: &DatabaseConnection,
    key: &str,
    value: Option<String>,
) -> Result<(), AppError> {
    let active = sys_config::ActiveModel {
        key: Set(key.to_string()),
        value: Set(value),
        ..Default::default()
    };

    if sys_config::Entity::insert(active.clone()).exec(db).await.is_err() {
        sys_config::Entity::update(active)
            .exec(db)
            .await
            .map_err(|_| AppError::system_exception())?;
    }
    Ok(())
}

fn require_admin(auth: &AuthUser) -> Result<(), AppError> {
    if auth.role.as_deref() != Some("ADMIN") {
        return Err(AppError::need_login());
    }
    Ok(())
}

const OPEN_REGISTER: &str = "OPEN_REGISTER";
const WEBSITE_TITLE: &str = "WEBSITE_TITLE";
const OPEN_COMMENT: &str = "OPEN_COMMENT";
const OPEN_LIKE: &str = "OPEN_LIKE";
const MEMO_MAX_LENGTH: &str = "MEMO_MAX_LENGTH";
const INDEX_WIDTH: &str = "INDEX_WIDTH";
const USER_MODEL: &str = "USER_MODEL";
const CUSTOM_CSS: &str = "CUSTOM_CSS";
const CUSTOM_JAVASCRIPT: &str = "CUSTOM_JAVASCRIPT";
const THUMBNAIL_SIZE: &str = "THUMBNAIL_SIZE";
const ANONYMOUS_COMMENT: &str = "ANONYMOUS_COMMENT";
const COMMENT_APPROVED: &str = "COMMENT_APPROVED";

const WEB_HOOK_TOKEN: &str = "WEB_HOOK_TOKEN";
