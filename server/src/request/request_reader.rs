use tokio::io::{AsyncRead, AsyncReadExt};

use super::error::Error;
use super::request::{DEFAULT_MAX_BODY_SIZE, ParseState, Request};

pub struct RequestReader<R> {
    reader: R,
    buffer: [u8; 8192],
    buffer_index: usize,
    max_body_size: usize,
}

impl<R: AsyncRead + Unpin> RequestReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: [0; 8192],
            buffer_index: 0,
            max_body_size: DEFAULT_MAX_BODY_SIZE,
        }
    }

    /// Largest request body accepted before failing with [`ParseError::RequestTooLarge`](super::ParseError::RequestTooLarge).
    pub fn max_body_size(mut self, max_body_size: usize) -> Self {
        self.max_body_size = max_body_size;
        self
    }

    /// Reads a complete request: head and body.
    pub async fn handle_request(&mut self) -> Result<Request, Error> {
        let request = self.read_head().await?;
        self.read_body(request).await
    }

    /// Reads the request line and headers. The body may already be partly or fully read.
    pub async fn read_head(&mut self) -> Result<Request, Error> {
        let mut request = Request::new();
        request.max_body_size = self.max_body_size;
        self.fill(&mut request, |request| {
            !matches!(request.parse_state, ParseState::Init | ParseState::Headers)
        })
        .await?;
        Ok(request)
    }

    /// Reads the rest of the body of a request returned by [`Self::read_head`].
    pub async fn read_body(&mut self, mut request: Request) -> Result<Request, Error> {
        self.fill(&mut request, Request::done).await?;
        Ok(request)
    }

    async fn fill(
        &mut self,
        request: &mut Request,
        until: fn(&Request) -> bool,
    ) -> Result<(), Error> {
        loop {
            if self.buffer_index > 0 {
                let read = request
                    .parse(&self.buffer[..self.buffer_index])
                    .map_err(Error::Parse)?;
                self.buffer.copy_within(read..self.buffer_index, 0);
                self.buffer_index -= read;
            }

            if until(request) {
                return Ok(());
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
