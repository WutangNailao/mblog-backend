use std::env;

#[derive(Clone)]
pub struct AppConfig {
    pub server_port: u16,
    pub db_type: String,
    pub mysql_url: String,
    pub mysql_db: String,
    pub mysql_user: String,
    pub mysql_pass: String,
    pub sqlite_path: String,
    pub database_url: Option<String>,
    pub jwt_secret: String,
    pub token_header: String,
    pub safe_domain: String,
    pub official_square_url: String,
    pub upload_storage_path: String,
    pub db_time_zone: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let server_port = env::var("SERVER_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(38321);

        let db_type = env::var("DB_TYPE").unwrap_or_default();
        let mysql_url = env::var("MYSQL_URL").unwrap_or_else(|_| "127.0.0.1".to_string());
        let mysql_db = env::var("MYSQL_DB").unwrap_or_else(|_| "memo".to_string());
        let mysql_user = env::var("MYSQL_USER").unwrap_or_else(|_| "tester".to_string());
        let mysql_pass = env::var("MYSQL_PASS").unwrap_or_else(|_| "tester".to_string());
        let sqlite_path = env::var("SQLITE_PATH").unwrap_or_else(|_| "/opt/mblog/data.sqlite".to_string());
        let database_url = env::var("DATABASE_URL").ok();

        let jwt_secret = env::var("SA_TOKEN_JWT_SECRET_KEY")
            .or_else(|_| env::var("JWT_SECRET"))
            .unwrap_or_else(|_| "6c6AJaXnTRXWpr9aUUqP".to_string());

        let token_header = env::var("SA_TOKEN_HEADER")
            .or_else(|_| env::var("TOKEN_HEADER"))
            .unwrap_or_else(|_| "token".to_string());

        let safe_domain = env::var("MBLOG_FRONT_DOMAIN").unwrap_or_default();
        let official_square_url = env::var("OFFICIAL_SQUARE_URL")
            .unwrap_or_else(|_| "https://square.mblog.club".to_string());
        let upload_storage_path = env::var("UPLOAD_STORAGE_PATH")
            .unwrap_or_else(|_| "/opt/mblog/upload".to_string());
        let db_time_zone = env::var("DB_TIME_ZONE")
            .unwrap_or_else(|_| "+08:00".to_string());

        Self {
            server_port,
            db_type,
            mysql_url,
            mysql_db,
            mysql_user,
            mysql_pass,
            sqlite_path,
            database_url,
            jwt_secret,
            token_header,
            safe_domain,
            official_square_url,
            upload_storage_path,
            db_time_zone,
        }
    }

    pub fn database_url(&self) -> String {
        if let Some(url) = &self.database_url {
            return url.clone();
        }

        if self.db_type.trim() == "-sqlite" {
            return format!("sqlite:{}", self.sqlite_path);
        }

        format!(
            "mysql://{}:{}@{}/{}",
            self.mysql_user, self.mysql_pass, self.mysql_url, self.mysql_db
        )
    }

    pub fn official_square_url(&self) -> String {
        self.official_square_url.clone()
    }

    pub fn upload_storage_path(&self) -> String {
        self.upload_storage_path.clone()
    }

    pub fn db_time_zone(&self) -> String {
        self.db_time_zone.clone()
    }
}
