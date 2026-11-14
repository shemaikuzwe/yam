use bench::{ADDR, check, load};

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    load(move |route| {
        let client = client.clone();
        async move {
            let res = client
                .get(format!("http://{ADDR}{route}"))
                .send()
                .await
                .unwrap();
            check(route, &res.bytes().await.unwrap());
        }
    })
    .await;
}
