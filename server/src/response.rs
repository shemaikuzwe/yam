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
}
impl<W: AsyncWrite + Unpin> Response<W> {
    pub fn new(writer: W) -> Self {
        let mut headers = Headers::new();
        headers.set("connection", "close".to_string());
        headers.set("content-type", "text/plain".to_string());
        Self {
            writer,
            status: StatusCode::StatusOk,
            headers,
        }
    }
    pub fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
    pub fn set(mut self, key: &str, value: &str) -> Self {
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
    pub async fn send(self, body: impl AsRef<[u8]>) -> io::Result<()> {
        let mut this = self;
        let body = body.as_ref();
        this.headers.set("content-length", body.len().to_string());
        this.write_status_line().await?;
        for (k, v) in this.headers.iter() {
            this.writer
                .write_all(format!("{k}: {v}\r\n").as_bytes())
                .await?;
        }
        this.writer.write_all(b"\r\n").await?;
        this.writer.write_all(body).await?;
        this.writer.flush().await?;
        Ok(())
    }
    pub async fn json(self, body: &impl Serialize) -> io::Result<()> {
        let body =
            serde_json::to_vec(body).map_err(|_| io::Error::new(InvalidData, "Invalid json"))?;
        self.set("content-type", "application/json").send(body).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn should_send_complete_response() {
        let mut output = Vec::new();
        Response::new(&mut output)
            .status(StatusCode::StatusOk)
            .set("content-type", "text/plain")
            .send("Hello")
            .await
            .unwrap();

        let output = String::from_utf8(output).unwrap();

        assert!(output.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(output.contains("content-type: text/plain\r\n"));
        assert!(output.contains("content-length: 5\r\n"));
        assert!(output.ends_with("\r\nHello"));
    }
    #[tokio::test]
    async fn should_send_binary_body() {
        let mut output = Vec::new();
        Response::new(&mut output)
            .set("content-type", "application/octet-stream")
            .send(vec![1, 2, 3, 4])
            .await
            .unwrap();

        assert!(output.ends_with(&[1, 2, 3, 4]));
    }
}
