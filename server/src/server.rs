use std::{future::Future, io, pin::Pin, sync::Arc, time::Duration};

use crate::{
    Error,
    request::{ConnectionType, Request, RequestReader, get_connection_type},
    response::{HttpError, IntoResponse, Response, ResponseWriter},
};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

pub struct Server;

pub type HandlerFuture = Pin<Box<dyn Future<Output = Result<Response, HttpError>> + Send>>;
pub trait Handler: Send + Sync + 'static {
    fn call(&self, req: Request) -> HandlerFuture;
}
impl<F, Fut, R> Handler for F
where
    F: Send + Sync + 'static,
    F: Fn(Request) -> Fut,
    Fut: Future<Output = R> + Send + 'static,
    R: IntoResponse + Send + 'static,
{
    fn call(&self, req: Request) -> HandlerFuture {
        let fut = self(req);
        Box::pin(async move { Ok(fut.await.into_response()) })
    }
}
impl Server {
    pub async fn serve<H>(listener: TcpListener, handler: H) -> io::Result<()>
    where
        H: Handler,
    {
        let handler = Arc::new(handler);
        loop {
            let (stream, _addr) = listener.accept().await?;
            // Disable Nagle so small responses aren't held back waiting on delayed ACKs.
            let _ = stream.set_nodelay(true);

            let handler = Arc::clone(&handler);

            tokio::spawn(async move { handle_request(stream, handler).await });
        }
    }
}

pub async fn handle_request(stream: TcpStream, handler: Arc<dyn Handler>) -> Result<(), HttpError> {
    let (read_stream, mut write_stream) = stream.into_split();

    let mut request_reader = RequestReader::new(read_stream);
    loop {
        let request = match timeout(Duration::from_secs(30), request_reader.handle_request()).await
        {
            Err(_elapsed) => return Ok(()), // Idle too long: close without an error response.
            Ok(Err(Error::GracefulEof)) => return Ok(()),
            Ok(Err(err)) => {
                let mut response = HttpError::from(err).into_response();
                response
                    .headers
                    .set("connection", ConnectionType::Close.to_string());
                ResponseWriter::new(&mut write_stream)
                    .send_response(response)
                    .await?;
                return Ok(());
            }
            Ok(Ok(request)) => request,
        };

        let connection_type = get_connection_type(&request);
        let is_head = request.method().eq_ignore_ascii_case("HEAD");

        let mut response = match handler.call(request).await {
            Ok(response) => response,
            Err(err) => err.into_response(),
        };
        response
            .headers
            .set("connection", connection_type.to_string());
        ResponseWriter::new(&mut write_stream)
            .head_only(is_head)
            .send_response(response)
            .await?;

        if connection_type == ConnectionType::Close {
            return Ok(());
        }
    }
}
