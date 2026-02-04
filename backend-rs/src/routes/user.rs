use actix_web::{web, HttpResponse};
use bcrypt::{hash, verify};
use chrono::{Duration, SecondsFormat, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};
use log::error;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::config::AppConfig;
use crate::entity::user;
use crate::error::AppError;
use crate::response::ResponseDto;
use crate::sys_config;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/register").route(web::post().to(register_user)),
    )
    .service(web::resource("/update").route(web::post().to(update_user)))
    .service(web::resource("/current").route(web::post().to(current_user)))
    .service(web::resource("/{id:\\d+}").route(web::post().to(get_user)))
    .service(web::resource("/list").route(web::post().to(list_users)))
    .service(web::resource("/login").route(web::post().to(login)))
    .service(web::resource("/logout").route(web::post().to(logout)))
    .service(web::resource("/listNames").route(web::post().to(list_names)))
    .service(web::resource("/statistics").route(web::post().to(statistics)));
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterUserRequest {
    username: Option<String>,
    password: Option<String>,
    display_name: Option<String>,
    email: Option<String>,
    bio: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserRequest {
    display_name: Option<String>,
    email: Option<String>,
    bio: Option<String>,
    avatar_url: Option<String>,
    password: Option<String>,
    default_visibility: Option<String>,
    default_enable_comment: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    username: Option<String>,
    password: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    token: String,
    username: String,
    role: Option<String>,
    user_id: i32,
    default_visibility: Option<String>,
    default_enable_comment: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserDto {
    id: i32,
    username: String,
    email: Option<String>,
    display_name: Option<String>,
    bio: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    role: Option<String>,
    avatar_url: Option<String>,
    default_visibility: Option<String>,
    default_enable_comment: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoStatisticsDto {
    total: i64,
    liked: i64,
    mentioned: i64,
    commented: i64,
    unread_mentioned: i64,
}

#[derive(Serialize)]
struct EmptyResponse {}

#[derive(Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "loginId")]
    login_id: i32,
    device: String,
    exp: usize,
}

async fn register_user(
    db: web::Data<DatabaseConnection>,
    payload: web::Json<RegisterUserRequest>,
) -> Result<HttpResponse, AppError> {
    let username = payload.username.clone().unwrap_or_default();
    let password = payload.password.clone().unwrap_or_default();
    if username.trim().is_empty() {
        return Err(AppError::param_error("username cannot be null"));
    }
    if password.trim().is_empty() {
        return Err(AppError::param_error("password cannot be null"));
    }

    let open_register = sys_config::get_boolean(db.get_ref(), "OPEN_REGISTER")
        .await
        .map_err(|_| AppError::system_exception())?;
    if !open_register {
        return Err(AppError::fail("当前不允许注册"));
    }

    let display_name = if let Some(name) = &payload.display_name {
        if !name.trim().is_empty() {
            Some(name.clone())
        } else {
            Some(username.clone())
        }
    } else {
        Some(username.clone())
    };

    let password_hash = hash(password, 10).map_err(|_| AppError::system_exception())?;
    let now = Utc::now();

    let user_model = user::ActiveModel {
        username: Set(username),
        password_hash: Set(password_hash),
        email: Set(payload.email.clone()),
        display_name: Set(display_name),
        bio: Set(payload.bio.clone()),
        created: Set(Some(now)),
        updated: Set(Some(now)),
        ..Default::default()
    };

    if let Err(err) = user_model.insert(db.get_ref()).await {
        let msg = err.to_string();
        if msg.contains("Duplicate") || msg.contains("UNIQUE") {
            return Err(AppError::fail("用户名或昵称已存在"));
        }
        return Err(AppError::system_exception());
    }

    Ok(HttpResponse::Ok().json(ResponseDto::<EmptyResponse>::success(None)))
}

async fn update_user(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    payload: web::Json<UpdateUserRequest>,
) -> Result<HttpResponse, AppError> {
    let mut active = user::ActiveModel {
        id: Set(auth.user_id),
        ..Default::default()
    };

    active.updated = Set(Some(Utc::now()));

    if let Some(v) = payload.display_name.clone() {
        active.display_name = Set(Some(v));
    }
    if let Some(v) = payload.email.clone() {
        active.email = Set(Some(v));
    }
    if let Some(v) = payload.bio.clone() {
        active.bio = Set(Some(v));
    }
    if let Some(v) = payload.avatar_url.clone() {
        active.avatar_url = Set(Some(v));
    }
    if let Some(v) = payload.default_visibility.clone() {
        active.default_visibility = Set(Some(v));
    }
    if let Some(v) = payload.default_enable_comment.clone() {
        active.default_enable_comment = Set(Some(v));
    }

    if let Some(password) = payload.password.clone() {
        if !password.trim().is_empty() {
            let hashed = hash(password, 10).map_err(|_| AppError::system_exception())?;
            active.password_hash = Set(hashed);
        }
    }

    user::Entity::update(active)
        .exec(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    Ok(HttpResponse::Ok().json(ResponseDto::<EmptyResponse>::success(None)))
}

async fn get_user(
    db: web::Data<DatabaseConnection>,
    _auth: AuthUser,
    path: web::Path<i32>,
) -> Result<HttpResponse, AppError> {
    let user = user::Entity::find_by_id(*path)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    let dto = user.map(to_user_dto);
    Ok(HttpResponse::Ok().json(ResponseDto::success(dto)))
}

async fn current_user(
    db: web::Data<DatabaseConnection>,
    auth: OptionalAuthUser,
) -> Result<HttpResponse, AppError> {
    let user = if let Some(auth) = auth.0 {
        user::Entity::find_by_id(auth.user_id)
            .one(db.get_ref())
            .await
            .map_err(|e| {
                error!("current_user find_by_id failed: {}", e);
                AppError::system_exception()
            })?
    } else {
        user::Entity::find()
            .filter(user::Column::Role.eq("ADMIN"))
            .one(db.get_ref())
            .await
            .map_err(|e| {
                error!("current_user find admin failed: {}", e);
                AppError::system_exception()
            })?
    };

    let dto = user.map(to_user_dto);
    Ok(HttpResponse::Ok().json(ResponseDto::success(dto)))
}

async fn list_users(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    if auth.role.as_deref() != Some("ADMIN") {
        return Err(AppError::need_login());
    }

    let users = user::Entity::find()
        .all(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;
    let list: Vec<UserDto> = users.into_iter().map(to_user_dto).collect();
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(list))))
}

async fn login(
    db: web::Data<DatabaseConnection>,
    config: web::Data<AppConfig>,
    payload: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    let username = payload.username.clone().unwrap_or_default();
    let password = payload.password.clone().unwrap_or_default();
    if username.trim().is_empty() {
        return Err(AppError::param_error("username cannot be null"));
    }
    if password.trim().is_empty() {
        return Err(AppError::param_error("password cannot be null"));
    }

    let user = user::Entity::find()
        .filter(user::Column::Username.eq(username.clone()))
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    let user = match user {
        Some(user) => user,
        None => return Err(AppError::fail("用户不存在")),
    };

    let ok = verify(password, &user.password_hash).map_err(|_| AppError::system_exception())?;
    if !ok {
        return Err(AppError::fail("密码不正确"));
    }

    let exp = (Utc::now() + Duration::days(365 * 100)).timestamp() as usize;
    let claims = Claims {
        login_id: user.id,
        device: "WEB".to_string(),
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|_| AppError::system_exception())?;

    let response = LoginResponse {
        token,
        username,
        role: user.role.clone(),
        user_id: user.id,
        default_visibility: user.default_visibility.clone(),
        default_enable_comment: user.default_enable_comment.clone(),
    };

    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(response))))
}

async fn logout(_auth: AuthUser) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ResponseDto::<EmptyResponse>::success(None)))
}

async fn list_names(
    db: web::Data<DatabaseConnection>,
    _auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let users = user::Entity::find()
        .all(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;
    let names: Vec<String> = users
        .into_iter()
        .filter_map(|u| u.display_name)
        .collect();
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(names))))
}

async fn statistics(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let total = count_total_memos(db.get_ref(), auth.user_id).await?;
    let liked = count_liked(db.get_ref(), auth.user_id).await?;
    let mentioned = count_mentioned(db.get_ref(), auth.user_id).await?;
    let commented = count_commented(db.get_ref(), auth.user_id).await?;
    let unread_mentioned = count_unread_mentioned(db.get_ref(), auth.user_id).await?;

    let dto = MemoStatisticsDto {
        total,
        liked,
        mentioned,
        commented,
        unread_mentioned,
    };
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(dto))))
}

fn to_user_dto(model: user::Model) -> UserDto {
    UserDto {
        id: model.id,
        username: model.username,
        email: model.email,
        display_name: model.display_name,
        bio: model.bio,
        created: model.created.map(to_rfc3339),
        updated: model.updated.map(to_rfc3339),
        role: model.role,
        avatar_url: model.avatar_url,
        default_visibility: model.default_visibility,
        default_enable_comment: model.default_enable_comment,
    }
}

fn to_rfc3339(dt: chrono::DateTime<chrono::Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, false)
}

async fn count_total_memos(db: &DatabaseConnection, user_id: i32) -> Result<i64, AppError> {
    count_by_sql(
        db,
        "SELECT COUNT(*) as cnt FROM t_memo WHERE user_id = ?",
        vec![sea_orm::Value::Int(Some(user_id))],
    )
    .await
}

async fn count_liked(db: &DatabaseConnection, user_id: i32) -> Result<i64, AppError> {
    count_by_sql(
        db,
        "SELECT COUNT(*) as cnt FROM t_user_memo_relation WHERE user_id = ? AND fav_type = 'LIKE'",
        vec![sea_orm::Value::Int(Some(user_id))],
    )
    .await
}

async fn count_commented(db: &DatabaseConnection, user_id: i32) -> Result<i64, AppError> {
    count_by_sql(
        db,
        "SELECT COUNT(1) as cnt FROM (SELECT DISTINCT memo_id FROM t_comment WHERE user_id = ?) x",
        vec![sea_orm::Value::Int(Some(user_id))],
    )
    .await
}

async fn count_mentioned(db: &DatabaseConnection, user_id: i32) -> Result<i64, AppError> {
    let pattern = format!("%#{},%", user_id);
    let sql = "SELECT COUNT(1) as cnt FROM (SELECT DISTINCT memo_id FROM t_comment WHERE mentioned_user_id LIKE ?) x";
    count_by_sql(db, sql, vec![sea_orm::Value::String(Some(Box::new(pattern)))]).await
}

async fn count_unread_mentioned(db: &DatabaseConnection, user_id: i32) -> Result<i64, AppError> {
    let user = user::Entity::find_by_id(user_id)
        .one(db)
        .await
        .map_err(|_| AppError::system_exception())?;
    let last_clicked = user
        .and_then(|u| u.last_clicked_mentioned)
        .unwrap_or_else(|| Utc::now() - Duration::days(365 * 100));

    let pattern = format!("%#{},%", user_id);
    let sql = "SELECT COUNT(*) as cnt FROM t_comment WHERE mentioned_user_id LIKE ? AND created >= ?";
    count_by_sql(
        db,
        sql,
        vec![
            sea_orm::Value::String(Some(Box::new(pattern))),
            sea_orm::Value::ChronoDateTimeUtc(Some(Box::new(last_clicked))),
        ],
    )
    .await
}

async fn count_by_sql(
    db: &DatabaseConnection,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Result<i64, AppError> {
    let backend = db.get_database_backend();
    let stmt = sea_orm::Statement::from_sql_and_values(backend, sql, values);
    let row = db
        .query_one(stmt)
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(AppError::system_exception)?;

    let cnt: i64 = row.try_get("", "cnt").unwrap_or(0);
    Ok(cnt)
}
