//! Raw hyper with no router: the "how far from the best" baseline.

use std::convert::Infallible;

use bench::{ADDR, HELLO, MESSAGE};
use http_body_util::Full;
use hyper::{
    Request, Response, StatusCode, body::Bytes, body::Incoming, header, server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

async fn handle(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path();
    let (content_type, body) = match path {
        "/" => ("text/plain", HELLO.to_string()),
        "/json" => ("application/json", serde_json::to_string(&MESSAGE).unwrap()),
        _ => match path.strip_prefix("/users/") {
            Some(id) => ("text/plain", id.to_string()),
            None => {
                let mut res = Response::new(Full::default());
                *res.status_mut() = StatusCode::NOT_FOUND;
                return Ok(res);
            }
        },
    };

    let res = Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .body(Full::new(Bytes::from(body)))
        .unwrap();
    Ok(res)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(ADDR).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service_fn(handle))
                .await;
        });
    }
}
