use sea_orm::{EntityTrait, DatabaseConnection, ColumnTrait, QueryFilter};

use crate::entity::sys_config;

pub async fn get_string(db: &DatabaseConnection, key: &str) -> Result<Option<String>, sea_orm::DbErr> {
    let config = sys_config::Entity::find_by_id(key.to_string())
        .one(db)
        .await?;
    Ok(config.map(|c| {
        if let Some(value) = c.value {
            if !value.is_empty() {
                return value;
            }
        }
        c.default_value.unwrap_or_default()
    }))
}

pub async fn get_boolean(db: &DatabaseConnection, key: &str) -> Result<bool, sea_orm::DbErr> {
    let value = get_string(db, key).await?;
    Ok(value.unwrap_or_default().to_lowercase() == "true")
}

pub async fn get_cors_domain_list(db: &DatabaseConnection) -> Result<Option<String>, sea_orm::DbErr> {
    let config = sys_config::Entity::find()
        .filter(sys_config::Column::Key.eq("CORS_DOMAIN_LIST"))
        .one(db)
        .await?;
    Ok(config.map(|c| {
        if let Some(value) = c.value {
            if !value.is_empty() {
                return value;
            }
        }
        c.default_value.unwrap_or_default()
    }))
}
