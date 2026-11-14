//! Raw hyper client with its connection pool: the "how far from the best" baseline.

use bench::{ADDR, check, load};
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

#[tokio::main]
async fn main() {
    let client = Client::builder(TokioExecutor::new()).build_http::<Empty<Bytes>>();
    load(move |route| {
        let client = client.clone();
        async move {
            let uri = format!("http://{ADDR}{route}").parse().unwrap();
            let res = client.get(uri).await.unwrap();
            check(route, &res.into_body().collect().await.unwrap().to_bytes());
        }
    })
    .await;
}
