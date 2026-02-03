mod auth;
mod config;
mod db;
mod entity;
mod error;
mod response;
mod routes;
mod sys_config;

use actix_web::{middleware, web, App, HttpServer};
use config::AppConfig;
use db::connect_db;
use log::info;
use response::json_error_handler;
use routes::{comment, memo, resource, rss, tag, token, user};
use routes::sys_config as sys_config_routes;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();
    let config = AppConfig::from_env();
    let db = connect_db(&config).await;
    sys_config_routes::init_defaults(&db).await;
    let server_port = config.server_port;

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(db.clone()))
            .app_data(web::JsonConfig::default().error_handler(json_error_handler))
            .wrap(middleware::Logger::default())
            .wrap(actix_web::middleware::from_fn(routes::cors::cors_handler))
            .service(web::scope("/api")
                .service(web::scope("/user").configure(user::config))
                .service(web::scope("/token").configure(token::config))
                .service(web::scope("/memo").configure(memo::config))
                .service(web::scope("/tag").configure(tag::config))
                .service(web::scope("/comment").configure(comment::config))
                .service(web::scope("/resource").configure(resource::config))
                .service(web::scope("/sysConfig").configure(sys_config_routes::config))
            )
            .service(web::scope("/rss").configure(rss::config))
    })
    .bind(("0.0.0.0", server_port))?;
    info!("server started at http://0.0.0.0:{}", server_port);
    server.run().await
}
