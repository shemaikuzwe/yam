use std::{println, vec};

use serde::Deserialize;
use serde_json::json;
use yam_client::client::{Body, Error, HttpClient, HttpClientConfig, RequestOptions};

#[derive(Debug, Deserialize)]
struct UserResponse {
    id: String,
    email: String,
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some("http://localhost:3000/api/v1/".into()),
        ..Default::default()
    });
    let user = json!({
        "email":"user1@gmail.com",
        "password":"1234"
    });
    let res: UserResponse = client
        .post(
            "/users",
            RequestOptions {
                headers: vec![("content-type".into(), "application/json".into())],
                body: Some(Body::json(&user)?),
                ..Default::default()
            },
        )
        .await?
        .json()?;
    println!("{:#?}", res);

    Ok(())
}
