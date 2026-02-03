use actix_web::{web, HttpResponse};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use reqwest::Client;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::entity::{sys_config, user};
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
        let active = sys_config::ActiveModel {
            key: Set(WEB_HOOK_TOKEN.to_string()),
            value: Set(Some(encoded)),
            ..Default::default()
        };
        let _ = sys_config::Entity::update(active).exec(db).await;
    }
}

async fn save(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    config: web::Data<AppConfig>,
    payload: web::Json<SaveSysConfigRequest>,
) -> Result<HttpResponse, AppError> {
    require_admin(&auth)?;

    let items = payload.items.clone().ok_or_else(|| AppError::param_error("items must not be null"))?;

    let push2square = items.iter().any(|r| r.key == PUSH_OFFICIAL_SQUARE && r.value.as_deref() == Some("true"));
    if push2square {
        let token = sys_config_store::get_string(db.get_ref(), WEB_HOOK_TOKEN)
            .await
            .map_err(|_| AppError::system_exception())?
            .unwrap_or_default();
        push_official_square(db.get_ref(), &config, &items, &token).await?;
    }

    for item in items {
        let active = sys_config::ActiveModel {
            key: Set(item.key),
            value: Set(item.value),
            ..Default::default()
        };
        sys_config::Entity::update(active)
            .exec(db.get_ref())
            .await
            .map_err(|_| AppError::system_exception())?;
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

async fn push_official_square(
    db: &DatabaseConnection,
    config: &AppConfig,
    items: &[SysConfigDto],
    token: &str,
) -> Result<(), AppError> {
    let admin = user::Entity::find()
        .filter(user::Column::Role.eq("ADMIN"))
        .one(db)
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("管理员不存在"))?;

    let backend_domain = items.iter().find(|r| r.key == DOMAIN).and_then(|r| r.value.clone());
    let cors_domain = items.iter().find(|r| r.key == CORS_DOMAIN_LIST).and_then(|r| r.value.clone());
    let embed = std::env::var("MBLOG_EMBED").unwrap_or_default();

    let mut website = None;
    if !embed.is_empty() {
        if let Some(domain) = backend_domain {
            website = Some(domain);
        }
    } else if let Some(cors) = cors_domain {
        if let Some(first) = cors.split(',').next() {
            website = Some(first.to_string());
        }
    }

    #[derive(Serialize)]
    struct Payload {
        token: String,
        author: String,
        #[serde(rename = "avatarUrl")]
        avatar_url: Option<String>,
        website: Option<String>,
    }

    let payload = Payload {
        token: token.to_string(),
        author: admin.display_name.unwrap_or(admin.username),
        avatar_url: admin.avatar_url,
        website,
    };

    let url = format!("{}/api/token", config.official_square_url());
    let client = Client::new();
    let resp = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|_| AppError::fail("连接广场异常,请查看后台日志"))?;

    if !resp.status().is_success() {
        return Err(AppError::fail("连接广场异常,请查看后台日志"));
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

const PUSH_OFFICIAL_SQUARE: &str = "PUSH_OFFICIAL_SQUARE";
const WEB_HOOK_TOKEN: &str = "WEB_HOOK_TOKEN";
const DOMAIN: &str = "DOMAIN";
const CORS_DOMAIN_LIST: &str = "CORS_DOMAIN_LIST";
