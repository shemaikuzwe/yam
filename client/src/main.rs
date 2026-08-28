use tokio::io;
use yam_client::client::HttpClient;

#[tokio::main]
async fn main() -> io::Result<()> {
    let client = HttpClient::default();
    client.get("http://localhost:3000/api/v1").await?;
    Ok(())
}
