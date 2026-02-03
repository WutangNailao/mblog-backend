use actix_web::{web, HttpResponse};
use rss::{ChannelBuilder, GuidBuilder, ItemBuilder};
use sea_orm::{ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Statement};

use crate::config::AppConfig;
use crate::entity::user;
use crate::error::AppError;
use crate::sys_config as sys_config_store;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("").route(web::get().to(get_rss)));
}

async fn get_rss(
    db: web::Data<DatabaseConnection>,
    _config: web::Data<AppConfig>,
) -> Result<HttpResponse, AppError> {
    let admin = user::Entity::find()
        .filter(user::Column::Role.eq("ADMIN"))
        .one(db.get_ref())
        .await
        .ok()
        .flatten();

    let title = sys_config_store::get_string(db.get_ref(), "WEBSITE_TITLE")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let domain = sys_config_store::get_string(db.get_ref(), "DOMAIN")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let items = query_latest_memos(db.get_ref()).await.unwrap_or_default();

    let channel = ChannelBuilder::default()
        .title(title)
        .link(domain.clone())
        .description(admin.and_then(|u| u.bio).unwrap_or_default())
        .items(items)
        .build();

    Ok(HttpResponse::Ok()
        .content_type("application/rss+xml; charset=utf-8")
        .body(channel.to_string()))
}

async fn query_latest_memos(db: &DatabaseConnection) -> Result<Vec<rss::Item>, AppError> {
    let stmt = Statement::from_string(
        db.get_database_backend(),
        "select id,content,created,updated,user_id,tags from t_memo where `status` = 'NORMAL' and `visibility` = 'PUBLIC' order by priority desc, created desc limit 20",
    );
    let rows = db
        .query_all(stmt)
        .await
        .map_err(|_| AppError::system_exception())?;

    let mut items = Vec::new();
    for row in rows {
        let id: i32 = row.try_get::<i32>("", "id").unwrap_or(0);
        let content: String = row.try_get::<String>("", "content").unwrap_or_default();
        let created: chrono::NaiveDateTime = row
            .try_get::<chrono::NaiveDateTime>("", "created")
            .unwrap_or_else(|_| chrono::Utc::now().naive_utc());
        let _updated: chrono::NaiveDateTime = row
            .try_get::<chrono::NaiveDateTime>("", "updated")
            .unwrap_or(created);
        let user_id: i32 = row.try_get::<i32>("", "user_id").unwrap_or(0);
        let tags: String = row.try_get::<String>("", "tags").unwrap_or_default();

        let author = user::Entity::find_by_id(user_id)
            .one(db)
            .await
            .map_err(|_| AppError::system_exception())?
            .and_then(|u| u.display_name)
            .unwrap_or_default();

        let link = sys_config_store::get_string(db, "DOMAIN")
            .await
            .map_err(|_| AppError::system_exception())?
            .unwrap_or_default();
        let link = format!("{}/memo/{}", link, id);

        let guid = GuidBuilder::default().value(link.clone()).permalink(true).build();

        let categories = tags
            .split(',')
            .filter(|s: &&str| !s.is_empty())
            .map(|t: &str| rss::CategoryBuilder::default().name(t.to_string()).build())
            .collect::<Vec<_>>();

        let mut builder = ItemBuilder::default();
        builder.title(Some(truncate(&content, 20)));
        builder.link(Some(link));
        builder.guid(Some(guid));
        builder.description(Some(content.clone()));
        builder.author(Some(author));
        builder.pub_date(Some(to_rfc2822(created)));
        builder.categories(categories);
        let item = builder.build();

        items.push(item);
    }

    Ok(items)
}

fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        input.to_string()
    } else {
        input.chars().take(max).collect()
    }
}

fn to_rfc2822(dt: chrono::NaiveDateTime) -> String {
    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc).to_rfc2822()
}
