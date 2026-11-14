use std::sync::Arc;

use bench::{ADDR, check, load};
use yam_http::client::{HttpClient, HttpClientConfig, RequestOptions};

#[tokio::main]
async fn main() {
    let client = Arc::new(HttpClient::new(HttpClientConfig {
        base_url: Some(format!("http://{ADDR}")),
        ..Default::default()
    }));
    load(move |route| {
        let client = client.clone();
        async move {
            let res = client.get(route, RequestOptions::default()).await.unwrap();
            check(route, res.bytes());
        }
    })
    .await;
}
