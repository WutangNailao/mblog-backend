use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    http::Method,
    http::header::{HeaderName, HeaderValue},
    middleware::Next,
    Error,
    HttpResponse,
};
 

pub async fn cors_handler<B>(
    req: ServiceRequest,
    next: Next<B>,
) -> Result<ServiceResponse<EitherBody<B>>, Error>
where
    B: MessageBody,
{
    let mut res = if req.method() == Method::OPTIONS {
        let res = HttpResponse::Ok().finish().map_into_right_body();
        req.into_response(res)
    } else {
        next.call(req).await?.map_into_left_body()
    };

    let headers = res.headers_mut();
    headers.insert(
        HeaderName::from_static("access-control-allow-origin"),
        HeaderValue::from_static("*"),
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-methods"),
        HeaderValue::from_static("POST, PUT, GET, OPTIONS, DELETE"),
    );
    headers.insert(
        HeaderName::from_static("access-control-max-age"),
        HeaderValue::from_static("86400"),
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-headers"),
        HeaderValue::from_static("Origin, X-Requested-With, Content-Type, Accept, token"),
    );
    headers.insert(
        HeaderName::from_static("cache-control"),
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    headers.insert(
        HeaderName::from_static("pragma"),
        HeaderValue::from_static("no-cache"),
    );

    Ok(res)
}
