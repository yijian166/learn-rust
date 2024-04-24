use axum::{extract::{Extension, Path}, http::{StatusCode, HeaderMap, HeaderValue}, routing::get, Router};
use percent_encoding::{percent_decode_str, percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use anyhow::Result;
use bytes::Bytes;
use lru::LruCache;
use std::{collections::hash_map::DefaultHasher, convert::TryInto, hash::{{Hash, Hasher}}, sync::Arc,num::NonZeroUsize};
use axum::routing::head;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tracing::{info, instrument};


mod pb;
mod engine;
use engine::{Engine, Photon};
use image::ImageFormat;

use pb::*;

// 参数使用 serde 做 Deserialize， axum会自动识别并解析
#[derive(Deserialize)]
struct Params {
    spec: String,
    url: String,
}

type Cache = Arc<Mutex<LruCache<u64, Bytes>>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cache: Cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(1024).unwrap())));
    let app = Router::new()
        .route("/image/:spec/:url", get(generate));
        //.layer(ServiceBuilder::new().layer(Extension(cache).into_inner())); //TODO: fix type

    let addr: String = "127.0.0.1:3000".parse().unwrap();
    tracing::debug!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn generate(Path(Params { spec, url }): Path<Params>, Extension(cache): Extension<Cache>) -> Result<(HeaderMap,Vec<u8>), StatusCode> {
    let url = percent_decode_str(&url).decode_utf8_lossy();
    // let url = "xx".to_string();
    let spec: ImageSpec = spec
        .as_str()
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let data  = retrieve_image(&url,cache)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // handle image
    let mut headers = HeaderMap::new();
    let mut engine: Photon = data
        .try_into()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    engine.apple(&spec.specs);

    let image = engine.generate(ImageFormat::Jpeg);

    headers.insert("content-type", HeaderValue::from_static("image/jpeg"));
    Ok((headers,image))
}


#[instrument(level = "info", skip(cache))]
async fn retrieve_image(url: &str, cache: Cache) -> Result<Bytes> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let key = hasher.finish();

    let g = &mut cache.lock().await;
    let data = match g.get(&key) {
        Some(v) => {
            info!("Match cache {}", key);
            v.to_owned()
        }
        None => {
            info!("retrieve url");
            let resp = reqwest::get(url).await?;
            let data = resp.bytes().await?;
            g.put(key, data.clone());
            data
        }
    };

    Ok(data)
}



fn print_test_url(url: &str) {
    use std::borrow::Borrow;
    let spec1 = Spec::new_resize(500,800, resize::SampleFilter::CatmullRow);
    let spec2 = Spec::new_watermark(20,20);
    let spec3 = Spec::new_filter(filter::Filter::Marine);
    let image_spec = ImageSpec::new(vec![spec1, spec2, spec3]);
    let s:String = image_spec.borrow().into();
    let test_image = percent_encode(url.as_bytes(), NON_ALPHANUMERIC).to_string();
    println!("test url: http://localhost:3000/image/{}/{}",s, test_image);
}