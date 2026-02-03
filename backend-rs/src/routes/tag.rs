use actix_web::{web, HttpResponse};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionError, TransactionTrait};
use serde::{Deserialize, Serialize};

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::entity::{memo, tag, user};
use crate::error::AppError;
use crate::response::ResponseDto;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/list").route(web::post().to(list)))
        .service(web::resource("/top10").route(web::post().to(top10)))
        .service(web::resource("/remove").route(web::post().to(remove)))
        .service(web::resource("/save").route(web::post().to(save)));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TagDto {
    id: i32,
    name: String,
    count: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveTagRequest {
    list: Option<Vec<TagUpdateDto>>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TagUpdateDto {
    id: i32,
    name: String,
}

#[derive(Deserialize)]
struct RemoveQuery {
    id: i32,
}

async fn list(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
) -> Result<HttpResponse, AppError> {
    let rows = tag::Entity::find()
        .filter(tag::Column::UserId.eq(auth.user_id))
        .all(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;
    let list = rows.into_iter().map(to_dto).collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(list))))
}

async fn top10(
    db: web::Data<DatabaseConnection>,
    auth: OptionalAuthUser,
) -> Result<HttpResponse, AppError> {
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

    let rows = tag::Entity::find()
        .filter(tag::Column::UserId.eq(user_id))
        .order_by_desc(tag::Column::MemoCount)
        .limit(10)
        .all(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    let list = rows.into_iter().map(to_dto).collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(list))))
}

async fn remove(
    db: web::Data<DatabaseConnection>,
    auth: AuthUser,
    query: web::Query<RemoveQuery>,
) -> Result<HttpResponse, AppError> {
    let _ = tag::Entity::delete_many()
        .filter(tag::Column::UserId.eq(auth.user_id))
        .filter(tag::Column::Id.eq(query.id))
        .filter(tag::Column::MemoCount.eq(0))
        .exec(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;
    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

async fn save(
    db: web::Data<DatabaseConnection>,
    _auth: AuthUser,
    payload: web::Json<SaveTagRequest>,
) -> Result<HttpResponse, AppError> {
    let items = payload.list.clone().ok_or_else(|| AppError::param_error("items"))?;

    db.transaction::<_, (), AppError>(|txn| {
        let items = items.clone();
        Box::pin(async move {
            for item in items {
                let old = tag::Entity::find_by_id(item.id)
                    .one(txn)
                    .await
                    .map_err(|_| AppError::system_exception())?
                    .ok_or_else(|| AppError::fail("tag不存在"))?;

                let active = tag::ActiveModel {
                    id: Set(item.id),
                    name: Set(item.name.clone()),
                    ..Default::default()
                };
                active
                    .update(txn)
                    .await
                    .map_err(|_| AppError::system_exception())?;

                let memos = query_all(
                    txn,
                    "select id,tags from t_memo where tags like ?",
                    vec![format!("%{},", old.name).into()],
                )
                .await?;

                for row in memos {
                    let memo_id: i32 = row.try_get("", "id").unwrap_or(0);
                    let tags: String = row.try_get("", "tags").unwrap_or_default();
                    let new_tags = tags.replacen(&format!("{},", old.name), &format!("{},", item.name), 1);
                    let memo_active = memo::ActiveModel {
                        id: Set(memo_id),
                        tags: Set(Some(new_tags)),
                        updated: Set(Some(Utc::now())),
                        ..Default::default()
                    };
                    memo_active
                        .update(txn)
                        .await
                        .map_err(|_| AppError::system_exception())?;
                }
            }
            Ok(())
        })
    })
    .await
    .map_err(map_tx_error)?;

    Ok(HttpResponse::Ok().json(ResponseDto::<()>::success(None)))
}

fn to_dto(model: tag::Model) -> TagDto {
    TagDto {
        id: model.id,
        name: model.name,
        count: model.memo_count.unwrap_or(0),
    }
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

fn map_tx_error(err: TransactionError<AppError>) -> AppError {
    match err {
        TransactionError::Connection(_) => AppError::system_exception(),
        TransactionError::Transaction(app) => app,
    }
}
