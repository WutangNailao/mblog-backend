use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ObjectCannedAcl;
use aws_sdk_s3::Client as S3Client;
use chrono::Utc;
use futures_util::StreamExt;
use md5::{Digest, Md5};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::auth::AuthUser;
use crate::config::AppConfig;
use crate::entity::resource;
use crate::error::AppError;
use crate::response::ResponseDto;
use crate::sys_config as sys_config_store;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/upload").route(web::post().to(upload)))
        .service(web::resource("/{public_id}").route(web::get().to(get_resource)));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadResourceResponse {
    public_id: String,
    url: String,
    suffix: String,
    storage_type: String,
    file_type: String,
    file_name: String,
}

async fn upload(
    db: web::Data<DatabaseConnection>,
    config: web::Data<AppConfig>,
    auth: AuthUser,
    mut payload: Multipart,
) -> Result<HttpResponse, AppError> {
    let storage_type = sys_config_store::get_string(db.get_ref(), "STORAGE_TYPE")
        .await
        .map_err(|_| AppError::system_exception())?
        .unwrap_or_else(|| "LOCAL".to_string());

    let mut responses = Vec::new();

    loop {
        let item = payload.next().await;
        let item = match item {
            Some(item) => item,
            None => break,
        };
        let mut field = match item {
            Ok(field) => field,
            Err(_) => return Err(AppError::fail("上传文件异常")),
        };
        let filename = field
            .content_disposition()
            .get_filename()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "file".to_string());

        let public_id = generate_public_id();
        let suffix = Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let parent_dir = Utc::now().format("%Y%m%d").to_string();
        let file_name = if suffix.is_empty() {
            public_id.clone()
        } else {
            format!("{}.{}", public_id, suffix)
        };
        let target_path = PathBuf::from(config.upload_storage_path())
            .join(parent_dir)
            .join(file_name);

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|_| AppError::fail("上传文件异常"))?;
        }

        let mut f = fs::File::create(&target_path).map_err(|_| AppError::fail("上传文件异常"))?;
        let mut hasher = Md5::new();
        let mut size: u64 = 0;

        loop {
            let chunk = field.next().await;
            let chunk = match chunk {
                Some(chunk) => chunk,
                None => break,
            };
            let data = match chunk {
                Ok(data) => data,
                Err(_) => return Err(AppError::fail("上传文件异常")),
            };
            size += data.len() as u64;
            hasher.update(&data);
            f.write_all(&data).map_err(|_| AppError::fail("上传文件异常"))?;
        }

        let file_hash = format!("{:x}", hasher.finalize());
        let file_type = detect_file_type(&target_path, &suffix);

        let (url, storage, suffix_from_cfg) = match storage_type.as_str() {
            "LOCAL" => (format!("/api/resource/{}", public_id), "LOCAL".to_string(), suffix.clone()),
            "QINIU" => {
                let qiniu_param = sys_config_store::get_string(db.get_ref(), "QINIU_PARAM")
                    .await
                    .map_err(|_| AppError::system_exception())?
                    .unwrap_or_default();
                if qiniu_param.trim().is_empty() || qiniu_param.trim() == "{}" {
                    let _ = fs::remove_file(&target_path);
                    return Err(AppError::fail("七牛云相关参数没有设置"));
                }
                let _ = fs::remove_file(&target_path);
                return Err(AppError::fail("上传资源失败"));
            }
            "AWSS3" => {
                let s3_param = sys_config_store::get_string(db.get_ref(), "AWSS3_PARAM")
                    .await
                    .map_err(|_| AppError::system_exception())?
                    .unwrap_or_default();
                let (url, suffix_cfg) = match upload_awss3(&s3_param, &target_path, &public_id).await {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = fs::remove_file(&target_path);
                        return Err(err);
                    }
                };
                (url, "AWSS3".to_string(), suffix_cfg)
            }
            _ => (format!("/api/resource/{}", public_id), "LOCAL".to_string(), suffix.clone()),
        };

        let now = Utc::now();
        let resource_model = resource::ActiveModel {
            public_id: Set(public_id.clone()),
            memo_id: Set(0),
            user_id: Set(auth.user_id),
            file_type: Set(file_type.clone()),
            file_name: Set(filename.clone()),
            file_hash: Set(file_hash),
            size: Set(size as i64),
            internal_path: Set(Some(target_path.to_string_lossy().to_string())),
            external_link: Set(Some(url.clone())),
            storage_type: Set(Some(storage.clone())),
            created: Set(Some(now)),
            updated: Set(Some(now)),
            suffix: Set(Some(suffix_from_cfg.clone())),
        };

        resource_model
            .insert(db.get_ref())
            .await
            .map_err(|_| AppError::system_exception())?;

        if storage != "LOCAL" {
            let _ = fs::remove_file(&target_path);
        }

        responses.push(UploadResourceResponse {
            public_id,
            url,
            suffix: suffix_from_cfg,
            storage_type: storage,
            file_type,
            file_name: filename,
        });
    }

    Ok(HttpResponse::Ok().json(ResponseDto::success(Some(responses))))
}

async fn get_resource(
    db: web::Data<DatabaseConnection>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let public_id = path.into_inner();
    let resource_item = resource::Entity::find_by_id(public_id.clone())
        .one(db.get_ref())
        .await
        .map_err(|_| AppError::system_exception())?;

    let resource_item = match resource_item {
        Some(r) => r,
        None => return Err(AppError::fail("resource不存在")),
    };

    let storage_type = resource_item.storage_type.as_deref().unwrap_or("LOCAL");
    if storage_type == "LOCAL" {
        let file_path = resource_item.internal_path.unwrap_or_default();
        let data = fs::read(&file_path).map_err(|_| AppError::fail("获取resource异常"))?;
        let file_type = resource_item.file_type;
        Ok(HttpResponse::Ok().content_type(file_type).body(data))
    } else {
        let url = resource_item.external_link.unwrap_or_default();
        Ok(HttpResponse::Found()
            .append_header(("Location", url))
            .finish())
    }
}

fn generate_public_id() -> String {
    let prefix = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let rand: String = (0..20)
        .map(|_| {
            let idx = rand::random::<u8>() % 26;
            (b'a' + idx) as char
        })
        .collect();
    format!("{}{}", prefix, rand)
}

fn detect_file_type(path: &Path, suffix: &str) -> String {
    if let Ok(kind) = infer::get_from_path(path) {
        if let Some(kind) = kind {
            return kind.mime_type().to_string();
        }
    }
    if !suffix.is_empty() {
        return format!("image/{}", suffix);
    }
    "application/octet-stream".to_string()
}

async fn upload_awss3(param: &str, file_path: &Path, public_id: &str) -> Result<(String, String), AppError> {
    let json: Value = serde_json::from_str(param).map_err(|_| AppError::fail("上传资源失败"))?;
    let access_key = json.get("accessKey").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let secret_key = json.get("secretKey").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let bucket = json.get("bucket").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let domain = json.get("domain").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let prefix = json.get("prefix").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let suffix = json.get("suffix").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let region = json.get("region").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if access_key.is_empty() || secret_key.is_empty() || bucket.is_empty() || region.is_empty() {
        return Err(AppError::fail("上传资源失败"));
    }

    let key = if prefix.is_empty() {
        public_id.to_string()
    } else {
        format!("{}/{}", prefix, public_id)
    };

    let region_provider = RegionProviderChain::first_try(Region::new(region.clone()));
    let creds = aws_sdk_s3::config::Credentials::new(
        access_key,
        secret_key,
        None,
        None,
        "static",
    );
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region_provider)
        .credentials_provider(creds)
        .load()
        .await;

    let client = S3Client::new(&config);
    let data = tokio::fs::read(file_path)
        .await
        .map_err(|_| AppError::fail("上传资源失败"))?;

    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .acl(ObjectCannedAcl::PublicRead)
        .body(ByteStream::from(data))
        .send()
        .await
        .map_err(|_| AppError::fail("上传资源失败"))?;

    let url = if !domain.is_empty() {
        format!("{}/{}", domain.trim_end_matches('/'), key)
    } else {
        format!("https://s3.{}.amazonaws.com/{}/{}", region, bucket, key)
    };

    Ok((url, suffix))
}
