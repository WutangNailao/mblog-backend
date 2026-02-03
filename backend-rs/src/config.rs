use std::env;

#[derive(Clone)]
pub struct AppConfig {
    pub server_port: u16,
    pub sqlite_path: String,
    pub database_url: Option<String>,
    pub jwt_secret: String,
    pub token_header: String,
    pub safe_domain: String,
    pub official_square_url: String,
    pub upload_storage_path: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let server_port = env::var("SERVER_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(38321);

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

        Self {
            server_port,
            sqlite_path,
            database_url,
            jwt_secret,
            token_header,
            safe_domain,
            official_square_url,
            upload_storage_path,
        }
    }

    pub fn database_url(&self) -> String {
        if let Some(url) = &self.database_url {
            return url.clone();
        }

        let path = self.sqlite_path.trim();
        if path.starts_with("sqlite:") || path.starts_with("file:") {
            return path.to_string();
        }
        format!("sqlite://{}", path)
    }

    pub fn official_square_url(&self) -> String {
        self.official_square_url.clone()
    }

    pub fn upload_storage_path(&self) -> String {
        self.upload_storage_path.clone()
    }

}
