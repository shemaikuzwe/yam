use std::{
    future::Future,
    io,
    pin::{Pin, pin},
    sync::Arc,
    time::Duration,
};

use crate::{
    Error,
    request::{ConnectionType, Request, RequestReader, get_connection_type},
    response::{HttpError, IntoResponse, Response, write_response},
};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, sleep};

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

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
    // Reused for every response on this connection.
    let mut write_buffer = Vec::with_capacity(512);
    let mut idle = pin!(sleep(IDLE_TIMEOUT));
    loop {
        // Pushing a live timer's deadline back is far cheaper than arming a new one per request.
        idle.as_mut().reset(Instant::now() + IDLE_TIMEOUT);
        let request = tokio::select! {
            biased;
            request = request_reader.handle_request() => request,
            // Idle too long: close without an error response.
            () = &mut idle => return Ok(()),
        };
        let request = match request {
            Err(Error::GracefulEof) => return Ok(()),
            Err(err) => {
                let mut response = HttpError::from(err).into_response();
                response
                    .headers
                    .set("connection", ConnectionType::Close.to_string());
                write_response(&mut write_stream, &mut write_buffer, response, false).await?;
                return Ok(());
            }
            Ok(request) => request,
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
        write_response(&mut write_stream, &mut write_buffer, response, is_head).await?;

        if connection_type == ConnectionType::Close {
            return Ok(());
        }
    }
}
