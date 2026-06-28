use std::io::{self, ErrorKind::InvalidData};

use serde::Serialize;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::headers::Headers;

#[derive(Debug)]
pub enum StatusCode {
    StatusOk = 200,
    StatusBadRequest = 400,
    StatusUnauthorized = 401,
    StatusForbidden = 403,
    StatusNotFound = 404,
    StatusInternalServerError = 500,
}
#[derive(Debug)]
pub struct Response<W: AsyncWrite> {
    status: StatusCode,
    writer: W,
    headers: Headers,
    sent: bool,
}
impl<W: AsyncWrite + Unpin> Response<W> {
    pub fn new(writer: W) -> Self {
        let mut h = Headers::new();
        h.set("connection", "close".to_string());
        h.set("content-type", "text/plain".to_string());
        Self {
            writer,
            status: StatusCode::StatusOk,
            headers: h,
            sent: false,
        }
    }
    pub fn status(&mut self, status: StatusCode) -> &mut Self {
        self.status = status;
        self
    }
    pub fn set(&mut self, key: &str, value: &str) -> &mut Self {
        self.headers.set(key, value.to_string());
        self
    }
    async fn write_status_line(&mut self) -> io::Result<()> {
        let (code, reason) = match self.status {
            StatusCode::StatusOk => (200, "OK"),
            StatusCode::StatusBadRequest => (400, "Bad Request"),
            StatusCode::StatusUnauthorized => (401, "Unauthorized"),
            StatusCode::StatusForbidden => (403, "Forbidden"),
            StatusCode::StatusNotFound => (404, "Not Found"),
            StatusCode::StatusInternalServerError => (500, "Internal Server Error"),
        };
        self.writer
            .write_all(format!("HTTP/1.1 {code} {reason}\r\n").as_bytes())
            .await?;
        Ok(())
    }
    pub async fn send(&mut self, body: impl AsRef<[u8]>) -> io::Result<()> {
        if self.sent {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "response already sent",
            ));
        }
        let body = body.as_ref();
        self.set("content-length", &body.len().to_string());
        self.write_status_line().await?;
        for (k, v) in self.headers.iter() {
            self.writer
                .write_all(format!("{k}: {v}\r\n").as_bytes())
                .await?;
        }
        self.writer.write_all(format!("\r\n").as_bytes()).await?;
        self.writer.write_all(body).await?;
        self.writer.flush().await?;
        self.sent = true;
        Ok(())
    }
    pub async fn json<T: Serialize>(&mut self, body: &T) -> io::Result<()> {
        let body =
            serde_json::to_vec(body).map_err(|_| io::Error::new(InvalidData, "Invalid json"))?;
        self.set("content-type", "application/json");
        self.send(body).await?;
        Ok(())
    }
    fn into_inner(self) -> W {
        self.writer
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn should_not_send_response_twice() {
        let mut res = Response::new(Vec::new());
        res.send("first").await.unwrap();
        let result = res.send("second").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn should_send_complete_response() {
        let mut res = Response::new(Vec::new());

        res.status(StatusCode::StatusOk)
            .set("content-type", "text/plain")
            .send("Hello")
            .await
            .unwrap();

        let output = String::from_utf8(res.into_inner()).unwrap();

        assert!(output.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(output.contains("content-type: text/plain\r\n"));
        assert!(output.contains("content-length: 5\r\n"));
        assert!(output.ends_with("\r\nHello"));
    }
    #[tokio::test]
    async fn should_send_binary_body() {
        let mut res = Response::new(Vec::new());

        res.set("content-type", "application/octet-stream")
            .send(vec![1, 2, 3, 4])
            .await
            .unwrap();

        let output = res.into_inner();

        assert!(output.ends_with(&[1, 2, 3, 4]));
    }
}
