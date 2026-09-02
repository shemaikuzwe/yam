use std::time::Duration;

use serde::{Deserialize, Serialize};
use yam_http::client::client::{Body, HttpClient, HttpClientConfig, RequestOptions};

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize)]
struct Post {
    userId: u32,
    id: u32,
    title: String,
    body: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some("https://jsonplaceholder.typicode.com".into()),
        timeout: Some(Duration::from_secs(10)),
        headers: vec![("accept".into(), "application/json".into())],
        ..Default::default()
    });

    let response = client.get("/posts/1", RequestOptions::default()).await?;
    if response.ok() {
        let post: Post = response.json()?;
        println!("[POST1] {:#?}", post);
    }

    let response = client.get("/users/1", RequestOptions::default()).await?;
    if response.ok() {
        let user: User = response.json()?;
        println!("[USER1] {:#?}", user);
    }

    // Absolute URL  overrides configured `base_url`
    let response = client
        .get(
            "https://jsonplaceholder.typicode.com/posts/2",
            RequestOptions::default(),
        )
        .await?;
    println!("[POST2] {}", response.text()?);

    let new_post = Post {
        body: "post3".into(),
        id: 2,
        title: "post3 title".into(),
        userId: 2,
    };
    let response = client
        .post(
            "/posts",
            RequestOptions {
                body: Some(Body::json(&new_post)?),
                ..Default::default()
            },
        )
        .await?;
    let created: Post = response.json()?;
    println!("[POST3]: {:#?}", created);

    Ok(())
}
