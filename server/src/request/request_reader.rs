use tokio::io::{AsyncRead, AsyncReadExt};

use super::error::Error;
use super::request::{ParseState, Request};

pub struct RequestReader<R> {
    reader: R,
    buffer: [u8; 8192],
    buffer_index: usize,
}

impl<R: AsyncRead + Unpin> RequestReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: [0; 8192],
            buffer_index: 0,
        }
    }

    pub async fn handle_request(&mut self) -> Result<Request, Error> {
        let mut request = Request::new();

        loop {
            if self.buffer_index > 0 {
                let read = request
                    .parse(&self.buffer[..self.buffer_index])
                    .map_err(Error::Parse)?;
                self.buffer.copy_within(read..self.buffer_index, 0);
                self.buffer_index -= read;

                if request.done() {
                    return Ok(request);
                }
            }

            if self.buffer_index == self.buffer.len() {
                return Err(Error::RequestTooLarge);
            }

            let bytes_read = self
                .reader
                .read(&mut self.buffer[self.buffer_index..])
                .await
                .map_err(Error::Io)?;

            if bytes_read == 0 {
                if self.buffer_index == 0 && request.parse_state == ParseState::Init {
                    return Err(Error::GracefulEof);
                }
                return Err(Error::UnexexpectedEndOfInput);
            }

            self.buffer_index += bytes_read;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::matches;

    use super::*;

    #[tokio::test]
    async fn reports_graceful_eof_before_a_request() {
        let mut reader = RequestReader::new(&b""[..]);
        assert!(matches!(
            reader.handle_request().await,
            Err(Error::GracefulEof)
        ));
    }

    #[tokio::test]
    async fn reports_unexpected_eof_for_a_partial_request() {
        let input = b"GET / HTTP/1.1\r\nHost: localhost\r\n";
        let mut reader = RequestReader::new(&input[..]);
        assert!(matches!(
            reader.handle_request().await,
            Err(Error::UnexexpectedEndOfInput)
        ));
    }
}
