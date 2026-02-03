use actix_web::{web, HttpResponse};
use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, Utc};
use log::{debug, error};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    Set, Statement, TransactionTrait, TransactionError,
};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::entity::{comment, memo, resource, tag, user, user_memo_relation};
use crate::error::AppError;
use crate::response::ResponseDto;
use crate::sys_config as sys_config_store;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/save").route(web::post().to(save)))
        .service(web::resource("/update").route(web::post().to(update)))
        .service(web::resource("/remove").route(web::post().to(remove)))
        .service(web::resource("/setPriority").route(web::post().to(set_priority)))
        .service(web::resource("/list").route(web::post().to(list)))
        .service(web::resource("/{id:\\d+}").route(web::post().to(get)))
        .service(web::resource("/statistics").route(web::post().to(statistics)))
        .service(web::resource("/relation").route(web::post().to(relation)));
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveMemoRequest {
    id: Option<i32>,
    content: Option<String>,
    public_ids: Option<Vec<String>>,
    visibility: Option<String>,
    enable_comment: Option<bool>,
    source: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListMemoRequest {
    page: Option<i64>,
    size: Option<i64>,
    tag: Option<String>,
    visibility: Option<String>,
    user_id: Option<i32>,
    begin: Option<String>,
    end: Option<String>,
    search: Option<String>,
    liked: Option<bool>,
    commented: Option<bool>,
    mentioned: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListMemoResponse {
    items: Vec<MemoDto>,
    total: i64,
    total_page: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoDto {
    id: i32,
    user_id: i32,
    content: Option<String>,
    tags: Option<String>,
    visibility: Option<String>,
    status: Option<String>,
    created: Option<String>,
    updated: Option<String>,
    author_name: Option<String>,
    author_role: Option<String>,
    email: Option<String>,
    bio: Option<String>,
    priority: i32,
    comment_count: i32,
    un_approved_comment_count: i64,
    like_count: i32,
    enable_comment: i32,
    view_count: i32,
    liked: i32,
    resources: Vec<ResourceDto>,
    source: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceDto {
    public_id: String,
    url: String,
    file_type: Option<String>,
    suffix: Option<String>,
    storage_type: Option<String>,
    file_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatisticsRequest {
    begin: Option<String>,
    end: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatisticsResponse {
    total_memos: i64,
    total_days: i64,
    total_tags: i64,
    items: Vec<StatisticsItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatisticsItem {
    date: String,
    total: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoRelationRequest {
    memo_id: i32,
    r#type: String,
    operate_type: String,
}

async fn save(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    payload: web::Json<SaveMemoRequest>,
) -> Result<HttpResponse, AppError> {
    let content = payload.content.clone().unwrap_or_default();
    let public_ids = payload.public_ids.clone().unwrap_or_default();
    check_content_and_resource(&content, &public_ids)?;

    let tags = parse_tags(&content);
    let visibility = payload
        .visibility
        .clone()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| Some("PUBLIC".to_string()));
    let memo_model = memo::ActiveModel {
        user_id: Set(auth.user_id),
        tags: Set(Some(format_tags(&tags))),
        visibility: Set(visibility),
        enable_comment: Set(Some(if payload.enable_comment.unwrap_or(false) { 1 } else { 0 })),
        content: Set(Some(replace_first_line(&content, &tags).trim().to_string())),
        created: Set(Some(Utc::now())),
        updated: Set(Some(Utc::now())),
        source: Set(payload.source.clone()),
        ..Default::default()
    };

    let result = db
        .transaction::<_, memo::Model, AppError>(|txn| {
            let tags_clone = tags.clone();
            let public_ids_clone = public_ids.clone();
            Box::pin(async move {
                let inserted = memo_model
                    .insert(txn)
                    .await
                    .map_err(|e| {
                        error!("memo insert failed: {}", e);
                        AppError::system_exception()
                    })?;
                debug!("memo saved id={}", inserted.id);
                sync_tags_on_save(txn, auth.user_id, &tags_clone).await?;
                debug!("memo tags synced id={}", inserted.id);
                if !public_ids_clone.is_empty() {
                    attach_resources(txn, inserted.id, &public_ids_clone).await?;
                    debug!("memo resources attached id={}", inserted.id);
                }
                Ok(inserted)
            })
        })
        .await
        .map_err(map_tx_error)?;

    let memo_id = result.id;
    notify_webhook_async(db.get_ref().clone(), memo_id);

    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(memo_id))))
}

async fn update(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    payload: web::Json<SaveMemoRequest>,
) -> Result<HttpResponse, AppError> {
    let id = payload.id.ok_or_else(|| AppError::param_error("memoID"))?;
    let content = payload.content.clone().unwrap_or_default();
    let public_ids = payload.public_ids.clone().unwrap_or_default();
    check_content_and_resource(&content, &public_ids)?;

    let exist = memo::Entity::find_by_id(id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("memo不存在"))?;

    let tags = parse_tags(&content);
    let old_tags = split_tags(exist.tags.clone());

    let visibility = payload
        .visibility
        .clone()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| exist.visibility.clone());
    let enable_comment = payload
        .enable_comment
        .map(|v| if v { 1 } else { 0 })
        .or(exist.enable_comment);

    let memo_model = memo::ActiveModel {
        id: Set(id),
        tags: Set(Some(format_tags(&tags))),
        content: Set(Some(replace_first_line(&content, &tags).trim().to_string())),
        enable_comment: Set(enable_comment),
        updated: Set(Some(Utc::now())),
        visibility: Set(visibility),
        created: Set(exist.created),
        source: Set(payload.source.clone().or(exist.source.clone())),
        ..Default::default()
    };

    db.transaction::<_, (), AppError>(|txn| {
        let tags_clone = tags.clone();
        let old_tags_clone = old_tags.clone();
        let public_ids_clone = public_ids.clone();
        Box::pin(async move {
            memo::Entity::update(memo_model)
                .exec(txn)
                .await
                .map_err(|_| AppError::system_exception())?;

            sync_tags_on_update(txn, auth.user_id, &tags_clone, &old_tags_clone).await?;
            clear_memo_resources(txn, id).await?;
            if !public_ids_clone.is_empty() {
                attach_resources(txn, id, &public_ids_clone).await?;
            }
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
    let memo_id = query.id;
    let memo_item = memo::Entity::find_by_id(memo_id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;
    if memo_item.is_none() {
        return Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)));
    }
    let memo_item = memo_item.unwrap();
    if auth.role.as_deref() != Some("ADMIN") && memo_item.user_id != auth.user_id {
        return Err(AppError::fail("不能删除其他人的记录"));
    }

    let tags = split_tags(memo_item.tags.clone());
    db.transaction::<_, (), AppError>(|txn| {
        let tags_clone = tags.clone();
        Box::pin(async move {
            for tag_name in tags_clone {
                decrement_tag_count(txn, auth.user_id, &tag_name).await?;
            }
            resource::Entity::delete_many()
                .filter(resource::Column::MemoId.eq(memo_id))
                .exec(txn)
                .await
                .map_err(|_| AppError::system_exception())?;
            memo::Entity::delete_by_id(memo_id)
                .exec(txn)
                .await
                .map_err(|_| AppError::system_exception())?;
            comment::Entity::delete_many()
                .filter(comment::Column::MemoId.eq(memo_id))
                .exec(txn)
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
struct SetPriorityQuery {
    id: i32,
    set: bool,
}

async fn set_priority(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    query: web::Query<SetPriorityQuery>,
) -> Result<HttpResponse, AppError> {
    let memo_item = memo::Entity::find_by_id(query.id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;
    if memo_item.is_none() {
        return Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)));
    }
    let memo_item = memo_item.unwrap();
    if auth.role.as_deref() != Some("ADMIN") && memo_item.user_id != auth.user_id {
        return Err(AppError::fail("不能操作其他人的记录"));
    }

    if query.set {
        let sql = "update t_memo set priority = ((select max(x.priority) from (select * from t_memo) as x)+1) where id = ?";
        exec_sql(db.get_ref(), sql, vec![memo_item.id.into()]).await?;
    } else {
        exec_sql(db.get_ref(), "update t_memo set priority = 0 where id = ?", vec![memo_item.id.into()]).await?;
    }

    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

#[derive(Deserialize)]
struct GetQuery {
    count: Option<bool>,
}

async fn get(
    db: web::Data<DatabaseConnection>,
    auth: OptionalAuthUser,
    path: web::Path<i32>,
    query: web::Query<GetQuery>,
) -> Result<HttpResponse, AppError> {
    let memo_id = *path;
    let is_login = auth.0.is_some();
    let mut conditions = Vec::new();
    conditions.push("t.id = ?".to_string());

    if is_login {
        conditions.push("(t.visibility in ('PUBLIC','PROTECT') or (t.visibility = 'PRIVATE' and t.user_id = ?))".to_string());
    } else {
        conditions.push("t.visibility = 'PUBLIC'".to_string());
    }

    let sql = format!("select t.* from t_memo t where {}", conditions.join(" and "));
    let mut values: Vec<sea_orm::Value> = vec![memo_id.into()];
    if is_login {
        values.push(auth.0.as_ref().unwrap().user_id.into());
    }
    let memo_row = query_one(db.get_ref(), &sql, values).await?;
    if memo_row.is_none() {
        return Ok(HttpResponse::Ok().json(ResponseDto::<MemoDto>::success(None)));
    }

    if query.count.unwrap_or(false) {
        exec_sql(db.get_ref(), "update t_memo set view_count = view_count + 1 where id = ?", vec![memo_id.into()]).await?;
    }

    let memo_item = row_to_memo_model(memo_row.unwrap());
    let dto = build_memo_dto(db.get_ref(), memo_item, auth.0.as_ref().map(|a| a.user_id)).await?;
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(dto))))
}

async fn list(
    db: web::Data<DatabaseConnection>,
    auth: OptionalAuthUser,
    payload: web::Json<ListMemoRequest>,
) -> Result<HttpResponse, AppError> {
    let page = payload.page.unwrap_or(1).max(1);
    let size = payload.size.unwrap_or(20).max(1);
    let offset = (page - 1) * size;

    let is_login = auth.0.is_some();
    let current_user_id = auth.0.as_ref().map(|a| a.user_id);
    let mut where_sql = vec!["t.status = 'NORMAL'".to_string()];
    let mut values = Vec::<sea_orm::Value>::new();

    if let Some(search) = payload.search.clone().filter(|s| !s.is_empty()) {
        where_sql.push("t.content like ?".to_string());
        values.push(format!("%{}%", search).into());
    }

    if let (Some(begin), Some(end)) = (payload.begin.clone(), payload.end.clone()) {
        if let (Ok(begin), Ok(end)) = (parse_date(&begin), parse_date(&end)) {
            where_sql.push("t.created between ? and ?".to_string());
            values.push(begin.into());
            values.push(end.into());
        }
    }

    if is_login {
        let uid = current_user_id.unwrap();
        where_sql.push("(t.visibility in ('PUBLIC','PROTECT') or (t.visibility = 'PRIVATE' and t.user_id = ?))".to_string());
        values.push(uid.into());
        if let Some(user_id) = payload.user_id {
            if user_id > 0 {
                where_sql.push("t.user_id = ?".to_string());
                values.push(user_id.into());
            }
        }
        if payload.liked.unwrap_or(false) {
            where_sql.push("tumr.memo_id = t.id and tumr.user_id = ? and tumr.fav_type = 'LIKE'".to_string());
            values.push(uid.into());
        }
        if payload.commented.unwrap_or(false) {
            where_sql.push("tc.memo_id = t.id".to_string());
            if payload.mentioned.unwrap_or(false) {
                where_sql.push("tc.mentioned_user_id like ?".to_string());
                values.push(format!("%#{},%", uid).into());
            } else {
                where_sql.push("tc.user_id = ?".to_string());
                values.push(uid.into());
            }
        }
    } else {
        where_sql.push("t.visibility = 'PUBLIC'".to_string());
        if let Some(user_id) = payload.user_id {
            if user_id > 0 {
                where_sql.push("t.user_id = ?".to_string());
                values.push(user_id.into());
            }
        }
    }

    if let Some(tag_value) = payload.tag.clone().filter(|v| !v.trim().is_empty()) {
        where_sql.push("t.tags like ?".to_string());
        values.push(format!("%{},%", tag_value).into());
    }

    if let Some(visibility) = payload.visibility.clone().filter(|v| !v.trim().is_empty()) {
        where_sql.push("t.visibility = ?".to_string());
        values.push(visibility.into());
    }

    let mut join_clause = String::new();
    if is_login && payload.liked.unwrap_or(false) {
        join_clause.push_str(", t_user_memo_relation tumr");
    }
    if is_login && payload.commented.unwrap_or(false) {
        join_clause.push_str(", t_comment tc");
    }

    let where_clause = where_sql.join(" and ");
    let count_sql = format!("select count(1) as cnt from t_memo t{} where {}", join_clause, where_clause);
    let total = query_count(db.get_ref(), &count_sql, values.clone()).await?;

    let mut order = String::new();
    if !payload.liked.unwrap_or(false) && !payload.commented.unwrap_or(false) && !payload.mentioned.unwrap_or(false) {
        order.push_str("t.priority desc, ");
    }

    let list_sql = format!(
        "select x.*,u.display_name as authorName,u.role as authorRole,u.email,u.bio,r.external_link as url,r.public_id as publicId,r.suffix,r.file_type as fileType,r.storage_type as storageType,r.file_name as fileName{} \
        from (select t.id,t.created,t.updated,t.content,t.priority,t.visibility,t.tags,t.status,t.user_id as userId,t.view_count as viewCount,t.enable_comment as enableComment,t.like_count as likeCount,t.comment_count as commentCount,t.source as source \
        from t_memo t{} where {} order by {} t.created desc limit ?,?) x \
        left join t_user u on u.id = x.userId \
        left join t_resource r on r.memo_id = x.id{} \
        order by {} x.created desc, r.created",
        if is_login {", mr.id as liked"} else {""},
        join_clause,
        where_clause,
        order,
        if is_login {format!(" left join t_user_memo_relation mr on mr.memo_id = x.id and mr.user_id = {} and mr.fav_type = 'LIKE'", current_user_id.unwrap())} else {"".to_string()},
        if !payload.liked.unwrap_or(false) && !payload.commented.unwrap_or(false) && !payload.mentioned.unwrap_or(false) {"x.priority desc,"} else {""},
    );

    values.push(offset.into());
    values.push(size.into());
    let rows = query_all(db.get_ref(), &list_sql, values).await?;
    let mut items = build_memo_list_from_rows(db.get_ref(), rows, is_login).await?;

    if is_login && payload.commented.unwrap_or(false) && payload.mentioned.unwrap_or(false) {
        if let Some(uid) = current_user_id {
            let mut u = user::ActiveModel { id: Set(uid), ..Default::default() };
            u.last_clicked_mentioned = Set(Some(Utc::now()));
            let _ = user::Entity::update(u).exec(db.get_ref()).await;
        }
    }

    let total_page = if total % size == 0 { total / size } else { total / size + 1 };
    let response = ListMemoResponse { items: items.drain(..).collect(), total, total_page };
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(response))))
}

async fn statistics(
    db: web::Data<DatabaseConnection>,
    auth: OptionalAuthUser,
    payload: web::Json<StatisticsRequest>,
) -> Result<HttpResponse, AppError> {
    let begin = payload.begin.clone().and_then(|b| parse_date(&b).ok());
    let end = payload.end.clone().and_then(|e| parse_date(&e).ok());

    let begin = begin.unwrap_or_else(|| (Utc::now() - Duration::days(50)).naive_utc());
    let end = end.unwrap_or_else(|| (Utc::now() + Duration::days(1)).naive_utc());
    if end < begin {
        return Err(AppError::param_error("end before begin"));
    }

    let user_id = if let Some(auth) = auth.0 {
        auth.user_id
    } else {
        let admin = user::Entity::find()
            .filter(user::Column::Role.eq("ADMIN"))
            .one(db.get_ref())
            .await
            .map_err(|_| AppError::system_exception())?
            .ok_or_else(|| AppError::fail("管理员不存在"))?;
        admin.id
    };

    let user_model = user::Entity::find_by_id(user_id)
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("用户不存在"))?;

    let total_memos = query_count(
        db.get_ref(),
        "select count(1) as cnt from t_memo where user_id = ?",
        vec![user_id.into()],
    )
    .await?;

    let total_days = if let Some(created) = user_model.created {
        let duration = Utc::now() - created;
        duration.num_days()
    } else {
        0
    };

    let total_tags = query_count(
        db.get_ref(),
        "select count(1) as cnt from t_tag where user_id = ?",
        vec![user_id.into()],
    )
    .await?;

    let stats_sql = "select date(created/1000,'unixepoch') as day,count(1) as count from t_memo where user_id = ? and created between ? and ? group by date(created/1000,'unixepoch') order by date(created/1000,'unixepoch') desc";

    let rows = query_all(
        db.get_ref(),
        stats_sql,
        vec![user_id.into(), begin.into(), end.into()],
    )
    .await?;

    let items = rows
        .into_iter()
        .map(|row| StatisticsItem {
            date: row.try_get("", "day").unwrap_or_default(),
            total: row.try_get("", "count").unwrap_or(0),
        })
        .collect();

    let response = StatisticsResponse {
        total_memos,
        total_days,
        total_tags,
        items,
    };

    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(response))))
}

async fn relation(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    payload: web::Json<MemoRelationRequest>,
) -> Result<HttpResponse, AppError> {
    let open_like = sys_config_store::get_boolean(db.get_ref(), "OPEN_LIKE")
        .await
        .map_err(|_| AppError::system_exception())?;
    if !open_like {
        return Err(AppError::fail("禁止点赞"));
    }

    if payload.operate_type == "ADD" {
        db.transaction::<_, (), AppError>(|txn| {
            let memo_id = payload.memo_id;
            let user_id = auth.user_id;
            let fav_type = payload.r#type.clone();
            Box::pin(async move {
                let count = query_count(
                    txn,
                    "select count(1) as cnt from t_user_memo_relation where memo_id = ? and user_id = ? and fav_type = ?",
                    vec![memo_id.into(), user_id.into(), fav_type.clone().into()],
                )
                .await?;
                if count > 0 {
                    return Err(AppError::fail("数据已存在"));
                }

                let relation = user_memo_relation::ActiveModel {
                    memo_id: Set(memo_id),
                    user_id: Set(user_id),
                    fav_type: Set(fav_type),
                    created: Set(Some(Utc::now())),
                    ..Default::default()
                };
                relation
                    .insert(txn)
                    .await
                    .map_err(|_| AppError::system_exception())?;

                exec_sql(
                    txn,
                    "update t_memo set like_count = like_count + 1 where id = ?",
                    vec![memo_id.into()],
                )
                .await?;
                Ok(())
            })
        })
        .await
        .map_err(map_tx_error)?;
    } else if payload.operate_type == "REMOVE" {
        db.transaction::<_, (), AppError>(|txn| {
            let memo_id = payload.memo_id;
            let user_id = auth.user_id;
            let fav_type = payload.r#type.clone();
            Box::pin(async move {
                let result = user_memo_relation::Entity::delete_many()
                    .filter(user_memo_relation::Column::MemoId.eq(memo_id))
                    .filter(user_memo_relation::Column::UserId.eq(user_id))
                    .filter(user_memo_relation::Column::FavType.eq(fav_type))
                    .exec(txn)
                    .await
                    .map_err(|_| AppError::system_exception())?;
                if result.rows_affected > 0 {
                    exec_sql(
                        txn,
                        "update t_memo set like_count = like_count - 1 where id = ? and like_count >= 1",
                        vec![memo_id.into()],
                    )
                    .await?;
                }
                Ok(())
            })
        })
        .await
        .map_err(map_tx_error)?;
    }

    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

fn check_content_and_resource(content: &str, public_ids: &[String]) -> Result<(), AppError> {
    if content.trim().is_empty() && public_ids.is_empty() {
        return Err(AppError::fail("内容和图片都为空"));
    }
    Ok(())
}

fn parse_tags(content: &str) -> Vec<String> {
    let mut lines = content.split('\n');
    let first_line = lines.next().unwrap_or("");
    if first_line.trim().is_empty() {
        return Vec::new();
    }
    first_line
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter(|s| s.starts_with('#') && s.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

fn replace_first_line(content: &str, tags: &[String]) -> String {
    if content.trim().is_empty() {
        return "".to_string();
    }
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    if lines.is_empty() {
        return "".to_string();
    }
    let mut first = lines[0].clone();
    for tag in tags {
        first = first.replace(&format!("{},", tag), "");
        first = first.replace(&format!("{} ", tag), "");
        first = first.replace(tag, "");
    }
    if first.trim().is_empty() {
        lines.remove(0);
    } else {
        lines[0] = first;
    }
    lines.join("\n")
}

fn format_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        return "".to_string();
    }
    format!("{}{},", "", tags.join(","))
}

fn split_tags(tags: Option<String>) -> Vec<String> {
    tags.unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

async fn sync_tags_on_save<C: ConnectionTrait>(
    db: &C,
    user_id: i32,
    tags: &[String],
) -> Result<(), AppError> {
    if tags.is_empty() {
        return Ok(());
    }
    let existing = tag::Entity::find()
        .filter(tag::Column::UserId.eq(user_id))
        .filter(tag::Column::Name.is_in(tags.iter().cloned()))
        .all(db)
        .await
        .map_err(|e| {
            error!("sync_tags_on_save find failed: {}", e);
            AppError::system_exception()
        })?;

    let mut new_tags: Vec<String> = tags.to_vec();
    for t in &existing {
        new_tags.retain(|x| x != &t.name);
    }

    for name in new_tags {
        let active = tag::ActiveModel {
            name: Set(name),
            user_id: Set(user_id),
            memo_count: Set(Some(1)),
            created: Set(Some(Utc::now())),
            updated: Set(Some(Utc::now())),
            ..Default::default()
        };
        active.insert(db).await.map_err(|e| {
            error!("sync_tags_on_save insert failed: {}", e);
            AppError::system_exception()
        })?;
    }

    for t in existing {
        increment_tag_count(db, user_id, &t.name).await?;
    }

    Ok(())
}

async fn sync_tags_on_update<C: ConnectionTrait>(
    db: &C,
    user_id: i32,
    new_tags: &[String],
    old_tags: &[String],
) -> Result<(), AppError> {
    sync_tags_on_save(db, user_id, new_tags).await?;
    for tag_name in old_tags {
        decrement_tag_count(db, user_id, tag_name).await?;
    }
    Ok(())
}

async fn increment_tag_count<C: ConnectionTrait>(db: &C, user_id: i32, name: &str) -> Result<(), AppError> {
    exec_sql(
        db,
        "update t_tag set memo_count = memo_count + 1 where user_id = ? and name = ?",
        vec![user_id.into(), name.into()],
    )
    .await
}

async fn decrement_tag_count<C: ConnectionTrait>(db: &C, user_id: i32, name: &str) -> Result<(), AppError> {
    exec_sql(
        db,
        "update t_tag set memo_count = memo_count - 1 where user_id = ? and name = ? and memo_count >= 1",
        vec![user_id.into(), name.into()],
    )
    .await
}

async fn attach_resources<C: ConnectionTrait>(
    db: &C,
    memo_id: i32,
    public_ids: &[String],
) -> Result<(), AppError> {
    for public_id in public_ids {
        exec_sql(
            db,
            "update t_resource set memo_id = ? where memo_id = 0 and public_id = ?",
            vec![memo_id.into(), public_id.clone().into()],
        )
        .await?;
    }
    Ok(())
}

async fn clear_memo_resources<C: ConnectionTrait>(db: &C, memo_id: i32) -> Result<(), AppError> {
    exec_sql(
        db,
        "update t_resource set memo_id = 0 where memo_id = ?",
        vec![memo_id.into()],
    )
    .await
}

async fn exec_sql<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<sea_orm::Value>) -> Result<(), AppError> {
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, sql, values);
    db.execute(stmt)
        .await
        .map_err(|e| {
            error!("exec_sql failed: {} (sql={})", e, sql);
            AppError::system_exception()
        })?;
    Ok(())
}

fn map_tx_error(err: TransactionError<AppError>) -> AppError {
    match err {
        TransactionError::Connection(_) => AppError::system_exception(),
        TransactionError::Transaction(app) => app,
    }
}

async fn query_one<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<sea_orm::Value>) -> Result<Option<sea_orm::QueryResult>, AppError> {
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, sql, values);
    db.query_one(stmt)
        .await
        .map_err(|_| AppError::system_exception())
}

async fn query_all<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<sea_orm::Value>) -> Result<Vec<sea_orm::QueryResult>, AppError> {
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, sql, values);
    db.query_all(stmt)
        .await
        .map_err(|_| AppError::system_exception())
}

async fn query_count<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<sea_orm::Value>) -> Result<i64, AppError> {
    let row = query_one(db, sql, values).await?;
    Ok(row
        .and_then(|r| r.try_get("", "cnt").ok())
        .unwrap_or(0))
}

fn parse_date(input: &str) -> Result<NaiveDateTime, AppError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.naive_utc());
    }
    if let Ok(ts) = input.parse::<i64>() {
        return Ok(DateTime::<Utc>::from_timestamp_millis(ts)
            .ok_or_else(|| AppError::param_error("时间格式错误"))?
            .naive_utc());
    }
    Err(AppError::param_error("时间格式错误"))
}

fn row_to_memo_model(row: sea_orm::QueryResult) -> memo::Model {
    memo::Model {
        id: row.try_get("", "id").unwrap_or(0),
        user_id: row.try_get("", "user_id").unwrap_or(0),
        content: row.try_get("", "content").ok(),
        tags: row.try_get("", "tags").ok(),
        visibility: row.try_get("", "visibility").ok(),
        status: row.try_get("", "status").ok(),
        created: get_datetime_utc(&row, "created"),
        updated: get_datetime_utc(&row, "updated"),
        priority: row.try_get("", "priority").ok(),
        comment_count: row.try_get("", "comment_count").ok(),
        like_count: row.try_get("", "like_count").ok(),
        enable_comment: row.try_get("", "enable_comment").ok(),
        view_count: row.try_get("", "view_count").ok(),
        source: row.try_get("", "source").ok(),
    }
}

async fn build_memo_dto(
    db: &DatabaseConnection,
    memo_item: memo::Model,
    current_user_id: Option<i32>,
) -> Result<MemoDto, AppError> {
    let user_model = user::Entity::find_by_id(memo_item.user_id)
        .one(db)
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("用户不存在"))?;

    let domain = sys_config_store::get_string(db, "DOMAIN")
        .await
        .map_err(|_| AppError::system_exception())?
        .unwrap_or_default();

    let resources = resource::Entity::find()
        .filter(resource::Column::MemoId.eq(memo_item.id))
        .all(db)
        .await
        .map_err(|_| AppError::system_exception())?;

    let resource_dto = resources
        .into_iter()
        .map(|r| convert_resource(&domain, r))
        .collect::<Vec<_>>();

    let unapproved_count = query_count(
        db,
        "select count(1) as cnt from t_comment where memo_id = ? and user_id < 0 and approved = 0",
        vec![memo_item.id.into()],
    )
    .await?;

    let liked = if let Some(uid) = current_user_id {
        let count = query_count(
            db,
            "select count(1) as cnt from t_user_memo_relation where memo_id = ? and user_id = ? and fav_type = 'LIKE'",
            vec![memo_item.id.into(), uid.into()],
        )
        .await?;
        if count > 0 { 1 } else { 0 }
    } else {
        0
    };

    Ok(MemoDto {
        id: memo_item.id,
        user_id: memo_item.user_id,
        content: memo_item.content,
        tags: memo_item.tags,
        visibility: memo_item.visibility,
        status: memo_item.status,
        created: memo_item.created.map(to_rfc3339_utc),
        updated: memo_item.updated.map(to_rfc3339_utc),
        author_name: user_model.display_name,
        author_role: user_model.role,
        email: user_model.email,
        bio: user_model.bio,
        priority: memo_item.priority.unwrap_or(0),
        comment_count: memo_item.comment_count.unwrap_or(0),
        un_approved_comment_count: unapproved_count,
        like_count: memo_item.like_count.unwrap_or(0),
        enable_comment: memo_item.enable_comment.unwrap_or(0),
        view_count: memo_item.view_count.unwrap_or(0),
        liked,
        resources: resource_dto,
        source: memo_item.source,
    })
}

async fn build_memo_list_from_rows(
    db: &DatabaseConnection,
    rows: Vec<sea_orm::QueryResult>,
    is_login: bool,
) -> Result<Vec<MemoDto>, AppError> {
    let mut map: std::collections::HashMap<i32, MemoDto> = std::collections::HashMap::new();
    let domain = sys_config_store::get_string(db, "DOMAIN")
        .await
        .map_err(|_| AppError::system_exception())?
        .unwrap_or_default();

    for row in rows {
        let memo_id: i32 = row.try_get("", "id").unwrap_or(0);
        let entry = map.entry(memo_id).or_insert_with(|| MemoDto {
            id: memo_id,
            user_id: row.try_get("", "userId").unwrap_or(0),
            content: row.try_get("", "content").ok(),
            tags: row.try_get("", "tags").ok(),
            visibility: row.try_get("", "visibility").ok(),
            status: row.try_get("", "status").ok(),
            created: get_naive_datetime(&row, "created").map(to_rfc3339_naive),
            updated: get_naive_datetime(&row, "updated").map(to_rfc3339_naive),
            author_name: row.try_get("", "authorName").ok(),
            author_role: row.try_get("", "authorRole").ok(),
            email: row.try_get("", "email").ok(),
            bio: row.try_get("", "bio").ok(),
            priority: row.try_get("", "priority").unwrap_or(0),
            comment_count: row.try_get("", "commentCount").unwrap_or(0),
            un_approved_comment_count: 0,
            like_count: row.try_get("", "likeCount").unwrap_or(0),
            enable_comment: row.try_get("", "enableComment").unwrap_or(0),
            view_count: row.try_get("", "viewCount").unwrap_or(0),
            liked: if is_login { if row.try_get::<Option<i32>>("", "liked").unwrap_or(None).is_some() { 1 } else { 0 } } else { 0 },
            resources: Vec::new(),
            source: row.try_get("", "source").ok(),
        });

        if let Ok(public_id) = row.try_get::<String>("", "publicId") {
            if !public_id.is_empty() {
                let resource_dto = ResourceDto {
                    public_id,
                    url: build_resource_url(&domain, row.try_get("", "url").ok(), row.try_get("", "storageType").ok()),
                    file_type: row.try_get("", "fileType").ok(),
                    suffix: row.try_get("", "suffix").ok(),
                    storage_type: row.try_get("", "storageType").ok(),
                    file_name: row.try_get("", "fileName").ok(),
                };
                entry.resources.push(resource_dto);
            }
        }
    }

    for memo in map.values_mut() {
        let count = query_count(
            db,
            "select count(1) as cnt from t_comment where memo_id = ? and user_id < 0 and approved = 0",
            vec![memo.id.into()],
        )
        .await?;
        memo.un_approved_comment_count = count;
    }

    Ok(map.into_values().collect())
}

fn convert_resource(domain: &str, r: resource::Model) -> ResourceDto {
    let url = build_resource_url(domain, r.external_link.clone(), r.storage_type.clone());
    ResourceDto {
        public_id: r.public_id,
        url,
        file_type: Some(r.file_type),
        suffix: r.suffix,
        storage_type: r.storage_type,
        file_name: Some(r.file_name),
    }
}

fn build_resource_url(domain: &str, external_link: Option<String>, storage_type: Option<String>) -> String {
    let link = external_link.unwrap_or_default();
    if storage_type.as_deref() == Some("LOCAL") {
        format!("{}{}", domain, link)
    } else {
        link
    }
}

fn to_rfc3339_naive(dt: NaiveDateTime) -> String {
    chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, false)
}

fn to_rfc3339_utc(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Millis, false)
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

fn get_datetime_utc(row: &sea_orm::QueryResult, col: &str) -> Option<DateTime<Utc>> {
    row.try_get::<DateTime<Utc>>("", col)
        .ok()
        .or_else(|| {
            row.try_get::<NaiveDateTime>("", col)
                .ok()
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        })
        .or_else(|| {
            row.try_get::<String>("", col)
                .ok()
                .and_then(parse_db_datetime)
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        })
}

fn parse_db_datetime(input: String) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(&input, "%Y-%m-%d %H:%M:%S").ok().or_else(|| {
        DateTime::parse_from_rfc3339(&input)
            .ok()
            .map(|dt| dt.naive_utc())
    })
}

fn to_millis(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

fn notify_webhook_async(db: DatabaseConnection, memo_id: i32) {
    actix_web::rt::spawn(async move {
        let _ = notify_webhook(&db, memo_id).await;
    });
}

async fn notify_webhook(db: &DatabaseConnection, memo_id: i32) -> Result<(), AppError> {
    let url = sys_config_store::get_string(db, "WEB_HOOK_URL")
        .await
        .map_err(|_| AppError::system_exception())?
        .unwrap_or_default();
    let token = sys_config_store::get_string(db, "WEB_HOOK_TOKEN")
        .await
        .map_err(|_| AppError::system_exception())?
        .unwrap_or_default();

    let memo_item = memo::Entity::find_by_id(memo_id)
        .one(db)
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("memo不存在"))?;
    if memo_item.visibility.as_deref() != Some("PUBLIC") || url.is_empty() {
        return Ok(());
    }

    let user_model = user::Entity::find_by_id(memo_item.user_id)
        .one(db)
        .await
        .map_err(|_| AppError::system_exception())?
        .ok_or_else(|| AppError::fail("用户不存在"))?;

    let resources = resource::Entity::find()
        .filter(resource::Column::MemoId.eq(memo_id))
        .all(db)
        .await
        .map_err(|_| AppError::system_exception())?;

    let backend_url = sys_config_store::get_string(db, "DOMAIN")
        .await
        .map_err(|_| AppError::system_exception())?
        .unwrap_or_default();

    let resource_urls = resources
        .into_iter()
        .map(|r| format!("{}/api/resource/{}", backend_url, r.public_id))
        .collect::<Vec<_>>();

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload {
        content: Option<String>,
        tags: Option<String>,
        created: i64,
        author_name: Option<String>,
        resources: Vec<String>,
    }

    let payload = Payload {
        content: memo_item.content.clone(),
        tags: memo_item.tags.clone(),
        created: memo_item.created.map(to_millis).unwrap_or(0),
        author_name: user_model.display_name.clone(),
        resources: resource_urls,
    };

    let client = reqwest::Client::new();
    let mut req = client.post(url).json(&payload);
    if !token.is_empty() {
        req = req.header("token", token);
    }
    let _ = req.send().await;
    Ok(())
}
