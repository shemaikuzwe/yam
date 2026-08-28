use yam_client::client::{Error, HttpClient, HttpClientConfig};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = HttpClient::new(HttpClientConfig {
        base_url: Some("http://localhost:3000/api/v1/".into()),
        ..Default::default()
    });
    let res = client.get("/users").await?;
    let text = res.text()?;
    println!("{text}");
    Ok(())
}
