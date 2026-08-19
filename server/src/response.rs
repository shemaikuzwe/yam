use std::io::{self, ErrorKind::InvalidData};

use serde::Serialize;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{headers::Headers, request};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusCode {
    StatusOk = 200,
    StatusBadRequest = 400,
    StatusUnauthorized = 401,
    StatusForbidden = 403,
    StatusNotFound = 404,
    StatusInternalServerError = 500,
}

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new() -> Self {
        let mut headers = Headers::new();
        headers.set("connection", "close".to_string());
        headers.set("content-type", "text/plain".to_string());
        Self {
            status: StatusCode::StatusOk,
            headers,
            body: Vec::new(),
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
    pub fn send(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
    pub fn json(self, body: &impl Serialize) -> Result<Self, serde_json::Error> {
        let body = serde_json::to_vec(body)?;
        Ok(self.set("content-type", "application/json").send(body))
    }
}

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl<R: IntoResponse> IntoResponse for Result<R, HttpError> {
    fn into_response(self) -> Response {
        match self {
            Ok(r) => r.into_response(),
            Err(e) => e.into_response(),
        }
    }
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response::new().send(self)
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Response::new().send(self)
    }
}

impl IntoResponse for serde_json::Value {
    fn into_response(self) -> Response {
        Response::new()
            .set("content-type", "application/json")
            .send(self.to_string())
    }
}

#[derive(Debug)]
pub struct ResponseWriter<W: AsyncWrite> {
    writer: W,
}

impl<W: AsyncWrite + Unpin> ResponseWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
    async fn write_status_line(&mut self, status: StatusCode) -> io::Result<()> {
        let (code, reason) = match status {
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
    pub async fn send_response(self, mut response: Response) -> io::Result<()> {
        let mut this = self;
        response
            .headers
            .set("content-length", response.body.len().to_string());
        this.write_status_line(response.status).await?;
        for (k, v) in response.headers.iter() {
            this.writer
                .write_all(format!("{k}: {v}\r\n").as_bytes())
                .await?;
        }
        this.writer.write_all(b"\r\n").await?;
        this.writer.write_all(&response.body).await?;
        this.writer.flush().await?;
        Ok(())
    }
    pub async fn send(self, body: impl AsRef<[u8]>) -> io::Result<()> {
        self.send_response(Response {
            body: body.as_ref().to_vec(),
            ..Response::new()
        })
        .await
    }
    pub async fn json(self, body: &impl Serialize) -> io::Result<()> {
        let response = Response::new()
            .json(body)
            .map_err(|_| io::Error::new(InvalidData, "Invalid json"))?;
        self.send_response(response).await
    }
}

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("Request error: {0}")]
    Request(#[from] request::Error),
    
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid JSON body: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid form body: {0}")]
    Form(#[from] serde_urlencoded::de::Error),
}

impl HttpError {
    pub fn status(&self) -> StatusCode {
        match self {
            HttpError::Io(_) => StatusCode::StatusInternalServerError,
            HttpError::Json(_) | HttpError::Form(_) | HttpError::Request(_) => {
                StatusCode::StatusBadRequest
            }
        }
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        Response::new().status(self.status()).send(self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn should_send_complete_response() {
        let mut output = Vec::new();
        ResponseWriter::new(&mut output)
            .send_response(
                Response::new()
                    .status(StatusCode::StatusOk)
                    .set("content-type", "text/plain")
                    .send("Hello"),
            )
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
        ResponseWriter::new(&mut output)
            .send_response(Response::new().send(vec![1, 2, 3, 4]))
            .await
            .unwrap();

        assert!(output.ends_with(&[1, 2, 3, 4]));
    }
}
