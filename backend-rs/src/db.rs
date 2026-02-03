use sea_orm::{ConnectionTrait, Database, DatabaseConnection, Statement};
use std::fs::{self, OpenOptions};
use std::path::Path;

use crate::config::AppConfig;

pub async fn connect_db(config: &AppConfig) -> DatabaseConnection {
    ensure_sqlite_path(config);
    let url = config.database_url();
    let db = Database::connect(&url)
        .await
        .unwrap_or_else(|e| panic!("db connect failed: {}", e));
    init_sqlite_schema(&db).await;
    db
}

fn ensure_sqlite_path(config: &AppConfig) {
    let raw = config.database_url();
    let path = raw
        .strip_prefix("sqlite://")
        .or_else(|| raw.strip_prefix("sqlite:"))
        .unwrap_or(raw.as_str());
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = OpenOptions::new()
        .create(true)
        .write(true)
        .open(path);
}

async fn init_sqlite_schema(db: &DatabaseConnection) {
    let backend = db.get_database_backend();
    let exists_stmt = Statement::from_string(
        backend,
        "SELECT name FROM sqlite_master WHERE type='table' AND name='t_sys_config' LIMIT 1",
    );
    let exists = db.query_one(exists_stmt).await.ok().flatten().is_some();
    if exists {
        return;
    }

    let sql = include_str!("../changelog-sqlite.sql");
    for stmt in split_sql(sql) {
        let _ = db
            .execute(Statement::from_string(backend, stmt))
            .await;
    }
}

fn split_sql(input: &str) -> Vec<String> {
    let mut buf = String::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--") || trimmed.is_empty() {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    buf.split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}
