use sea_orm::{ConnectionTrait, Database, DatabaseConnection};

use crate::config::AppConfig;

pub async fn connect_db(config: &AppConfig) -> DatabaseConnection {
    let url = config.database_url();
    let db = Database::connect(&url)
        .await
        .unwrap_or_else(|e| panic!("db connect failed: {}", e));
    let tz = config.db_time_zone();
    let _ = db
        .execute(sea_orm::Statement::from_string(
            db.get_database_backend(),
            format!("SET time_zone = '{}'", tz),
        ))
        .await;
    db
}
