use actix_web::{web, HttpResponse};
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use regex::Regex;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    Statement, TransactionError, TransactionTrait,
};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::entity::{comment, memo, user};
use crate::error::AppError;
use crate::response::ResponseDto;
use crate::sys_config as sys_config_store;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/add").route(web::post().to(add)))
        .service(web::resource("/remove").route(web::post().to(remove)))
        .service(web::resource("/query").route(web::post().to(query)))
        .service(web::resource("/singleApprove").route(web::post().to(single_approve)))
        .service(web::resource("/memoApprove").route(web::post().to(memo_approve)));
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveCommentRequest {
    content: String,
    memo_id: i32,
    username: Option<String>,
    email: Option<String>,
    link: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryCommentListRequest {
    page: i64,
    size: i64,
    memo_id: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryCommentListResponse {
    total: i64,
    total_page: i64,
    list: Vec<CommentDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommentDto {
    id: i32,
    memo_id: i32,
    user_name: String,
    user_id: i32,
    created: Option<String>,
    updated: Option<String>,
    content: String,
    mentioned: Option<String>,
    mentioned_user_id: Option<String>,
    email: Option<String>,
    link: Option<String>,
    approved: i32,
}

async fn add(
    db: web::Data<DatabaseConnection>,
    auth: OptionalAuthUser,
    payload: web::Json<SaveCommentRequest>,
) -> Result<HttpResponse, AppError> {
    let memo_item = memo::Entity::find_by_id(payload.memo_id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("memo不存在"))?;

    let open_comment = sys_config_store::get_boolean(db.get_ref(), "OPEN_COMMENT")
        .await
        .map_err(|_| AppError::system_exception())?;
    if !open_comment || memo_item.enable_comment.unwrap_or(0) != 1 {
        return Err(AppError::fail("禁止评论"));
    }

    let mut user_id = -1;
    let mut author_name = payload.username.clone().unwrap_or_default();
    if let Some(ref auth) = auth.0 {
        let user_model = user::Entity::find_by_id(auth.user_id)
            .one(db.get_ref())
            .await
            .map_err(|_| AppError::system_exception())?
            .ok_or_else(|| AppError::fail("用户不存在"))?;
        user_id = user_model.id;
        author_name = user_model.display_name.unwrap_or(user_model.username);
    } else {
        let anonymous = sys_config_store::get_boolean(db.get_ref(), "ANONYMOUS_COMMENT")
            .await
            .map_err(|_| AppError::system_exception())?;
        if !anonymous {
            return Err(AppError::fail("不支持匿名评论"));
        }
    }

    let comment_approved = sys_config_store::get_boolean(db.get_ref(), "COMMENT_APPROVED")
        .await
        .map_err(|_| AppError::system_exception())?;

    let (mentioned_names, mentioned_ids) = parse_mentions(db.get_ref(), &payload.content).await?;
    let mut comment_model = comment::ActiveModel {
        content: Set(payload.content.clone()),
        memo_id: Set(payload.memo_id),
        user_id: Set(user_id),
        user_name: Set(author_name),
        mentioned: Set(mentioned_names.clone()),
        mentioned_user_id: Set(mentioned_ids.clone()),
        created: Set(Some(Utc::now())),
        updated: Set(Some(Utc::now())),
        ..Default::default()
    };

    if auth.0.is_none() {
        comment_model.email = Set(payload.email.clone());
        comment_model.link = Set(payload.link.clone());
        comment_model.approved = Set(Some(if comment_approved { 0 } else { 1 }));
    }

    db.transaction::<_, (), AppError>(|txn| {
        let comment_model = comment_model.clone();
        Box::pin(async move {
            exec_sql(
                txn,
                "update t_memo set comment_count = comment_count + 1 where id = ?",
                vec![payload.memo_id.into()],
            )
            .await?;
            comment_model
                .insert(txn)
                .await
                .map_err(|_| AppError::system_exception())?;
            Ok(())
        })
    })
    .await
    .map_err(map_tx_error)?;

    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

#[derive(Deserialize)]
struct RemoveQuery {
    id: i32,
}

async fn remove(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    query: web::Query<RemoveQuery>,
) -> Result<HttpResponse, AppError> {
    let user_model = user::Entity::find_by_id(auth.user_id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("用户不存在"))?;

    let comment_model = comment::Entity::find_by_id(query.id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("评论不存在"))?;

    let memo_item = memo::Entity::find_by_id(comment_model.memo_id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("memo不存在"))?;

    if user_model.role.as_deref() != Some("ADMIN") && memo_item.user_id != user_model.id {
        return Err(AppError::fail("只能删除自己发的memo的评论"));
    }

    comment::Entity::delete_by_id(query.id)
        .exec(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

async fn query(
    db: web::Data<DatabaseConnection>,
    auth: OptionalAuthUser,
    payload: web::Json<QueryCommentListRequest>,
) -> Result<HttpResponse, AppError> {
    let page = payload.page.max(1);
    let size = payload.size.max(1);
    let offset = (page - 1) * size;

    let mut where_sql = vec!["memo_id = ?".to_string()];
    let values: Vec<sea_orm::Value> = vec![payload.memo_id.into()];

    let is_admin = auth.0.as_ref().and_then(|a| a.role.clone()).as_deref() == Some("ADMIN");
    if !is_admin {
        where_sql.push("(user_id > 0 or (user_id < 0 and approved = 1))".to_string());
    }

    let where_clause = where_sql.join(" and ");
    let count_sql = format!("select count(1) as cnt from t_comment where {}", where_clause);
    let total = query_count(db.get_ref(), &count_sql, values.clone()).await?;

    let list_sql = format!(
        "select * from t_comment where {} order by created limit {},{}",
        where_clause, offset, size
    );
    let rows = query_all(db.get_ref(), &list_sql, values).await?;

    let list = rows
        .into_iter()
        .map(|row| CommentDto {
            id: row.try_get("", "id").unwrap_or(0),
            memo_id: row.try_get("", "memo_id").unwrap_or(0),
            user_name: row.try_get("", "user_name").unwrap_or_default(),
            user_id: row.try_get("", "user_id").unwrap_or(0),
            created: get_naive_datetime(&row, "created").map(to_rfc3339),
            updated: get_naive_datetime(&row, "updated").map(to_rfc3339),
            content: row.try_get("", "content").unwrap_or_default(),
            mentioned: row.try_get("", "mentioned").ok(),
            mentioned_user_id: row.try_get("", "mentioned_user_id").ok(),
            email: row.try_get("", "email").ok(),
            link: row.try_get("", "link").ok(),
            approved: row.try_get("", "approved").unwrap_or(0),
        })
        .collect::<Vec<_>>();

    let total_page = if total % size == 0 { total / size } else { total / size + 1 };
    let response = QueryCommentListResponse { total, total_page, list };
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(response))))
}

#[derive(Deserialize)]
struct ApproveQuery {
    id: i32,
}

async fn single_approve(
    db: web::Data<DatabaseConnection>,
    query: web::Query<ApproveQuery>,
) -> Result<HttpResponse, AppError> {
    exec_sql(
        db.get_ref(),
        "update t_comment set approved = 1 where id = ? and user_id < 0",
        vec![query.id.into()],
    )
    .await?;
    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

async fn memo_approve(
    db: web::Data<DatabaseConnection>,
    query: web::Query<ApproveQuery>,
) -> Result<HttpResponse, AppError> {
    exec_sql(
        db.get_ref(),
        "update t_comment set approved = 1 where memo_id = ? and user_id < 0",
        vec![query.id.into()],
    )
    .await?;
    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

async fn parse_mentions(
    db: &DatabaseConnection,
    content: &str,
) -> Result<(Option<String>, Option<String>), AppError> {
    let regex = Regex::new(r"(@.*?)\\s+").map_err(|_| AppError::system_exception())?;
    let mut names = Vec::new();
    let mut ids = Vec::new();

    for cap in regex.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            let mut username = m.as_str().trim().to_string();
            if username.starts_with('@') {
                username.remove(0);
            }
            if username.is_empty() {
                continue;
            }
            let user_model = user::Entity::find()
                .filter(user::Column::DisplayName.eq(username.clone()))
                .one(db)
                .await
                .map_err(|_| AppError::system_exception())?;
            if let Some(u) = user_model {
                names.push(u.display_name.unwrap_or(u.username));
                ids.push(u.id.to_string());
            }
        }
    }

    let names_join = if names.is_empty() { None } else { Some(names.join(",")) };
    let ids_join = if ids.is_empty() {
        Some("".to_string())
    } else {
        Some(format!("#{}", ids.join(",#")) + ",")
    };

    Ok((names_join, ids_join))
}

async fn exec_sql<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Result<(), AppError> {
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, sql, values);
    db.execute(stmt)
        .await
        .map_err(|_| AppError::system_exception())?;
    Ok(())
}

async fn query_all<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Result<Vec<sea_orm::QueryResult>, AppError> {
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, sql, values);
    db.query_all(stmt)
        .await
        .map_err(|_| AppError::system_exception())
}

async fn query_count<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Result<i64, AppError> {
    let row = query_all(db, sql, values).await?;
    Ok(row
        .get(0)
        .and_then(|r| r.try_get("", "cnt").ok())
        .unwrap_or(0))
}

fn map_tx_error(err: TransactionError<AppError>) -> AppError {
    match err {
        TransactionError::Connection(_) => AppError::system_exception(),
        TransactionError::Transaction(app) => app,
    }
}

fn to_rfc3339(dt: chrono::NaiveDateTime) -> String {
    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, false)
}

fn get_naive_datetime(row: &sea_orm::QueryResult, col: &str) -> Option<NaiveDateTime> {
    row.try_get::<NaiveDateTime>("", col)
        .ok()
        .or_else(|| row.try_get::<DateTime<Utc>>("", col).ok().map(|dt| dt.naive_utc()))
        .or_else(|| {
            row.try_get::<String>("", col)
                .ok()
                .and_then(parse_db_datetime)
        })
}

fn parse_db_datetime(input: String) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(&input, "%Y-%m-%d %H:%M:%S").ok().or_else(|| {
        DateTime::parse_from_rfc3339(&input)
            .ok()
            .map(|dt| dt.naive_utc())
    })
}
