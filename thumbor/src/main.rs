use anyhow::Ok;
use axum::{extract::Path, routing::get, http::StatusCode, Router};
use percent_encoding::percent_decode_str;
use serde::Deserialize;
use std::convert::TryInto;

mod pb;

use pb::*;

// 参数使用 serde 做 Deserialize， axum会自动识别并解析
#[derive(Deserialize)]
struct Params {
    spec: String,
    url: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/image/:spec/:url", get(generate));

    let addr:String = "127.0.0.1:3000".parse().unwrap();
    tracing::debug!("listening on {}",addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}


async fn generate(Path(Params {spec, url}): Path<Params>) -> Result<String,StatusCode> {
    let url = percent_decode_str(&url).decode_utf8_lossy();
    //.try_into().map_err(|_| StatusCode::BAD_REQUEST)?;
    //.try_into().map_err(|_| StatusCode::BAD_REQUEST)?;
    let spec:ImageSpec = spec
        .as_str()
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let res = format!("url: {}\n spec: {:#?}",url, spec);
    //.try_into().map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(res)
}
