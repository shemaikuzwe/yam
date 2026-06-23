use std::io::{self, ErrorKind::InvalidData, Write};

use serde::Serialize;

use crate::headers::{Headers};

pub enum StatusCode {
    StatusOk = 200,
    StatusBadRequest = 400,
    StatusUnauthorized = 401,
    StatusForbidden = 403,
    StatusNotFound = 404,
    StatusInternalServerError = 500,
}
pub struct Response<W: Write> {
    status: StatusCode,
    writer: W,
    headers: Headers,
    sent: bool,
}
impl<W: Write> Response<W> {
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
    fn write_status_line(&mut self) -> io::Result<()> {
        let (code, reason) = match self.status {
            StatusCode::StatusOk => (200, "OK"),
            StatusCode::StatusBadRequest => (400, "Bad Request"),
            StatusCode::StatusUnauthorized => (401, "Unauthorized"),
            StatusCode::StatusForbidden => (403, "Forbidden"),
            StatusCode::StatusNotFound => (404, "Not Found"),
            StatusCode::StatusInternalServerError => (500, "Internal Server Error"),
        };

        write!(self.writer, "HTTP/1.1 {code} {reason}\r\n")?;

        Ok(())
    }
    pub fn send(&mut self, body: impl AsRef<[u8]>) -> io::Result<()> {
        if self.sent {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "response already sent",
            ));
        }
        let body = body.as_ref();
        self.set("content-length", &body.len().to_string());
        self.write_status_line()?;
        for (k, v) in self.headers.iter() {
            write!(self.writer, "{k}: {v}\r\n")?;
        }
        write!(self.writer, "\r\n")?;
        self.writer.write_all(body)?;
        self.writer.flush()?;
        self.sent = true;
        Ok(())
    }
    pub fn json<T: Serialize>(&mut self, body: &T) -> io::Result<()> {
        let body =
            serde_json::to_vec(body).map_err(|_| io::Error::new(InvalidData, "Invalid json"))?;
        self.set("content-type", "application/json");
        self.send(body)
    }
}
#[cfg(test)]
mod tests{
    // use super::*;
}