use yam_client::client::{Error, HttpClient};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = HttpClient::default();
    let res = client.get("http://localhost:3000/api/v1").await?;
    if res.ok() {
        let text = res.text()?;
        println!("{text}")
    }
    Ok(())
}
