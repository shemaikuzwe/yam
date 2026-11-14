use std::{future::Future, io, pin::Pin, sync::Arc, time::Duration};

use crate::{
    Error,
    request::{ConnectionType, DEFAULT_MAX_BODY_SIZE, Request, RequestReader, get_connection_type},
    response::{HttpError, IntoResponse, Response, ResponseWriter},
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
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

pub struct ServerConfig {
    /// How long a connection may take to send its next complete request
    /// before it is closed.
    pub read_timeout: Duration,
    /// Largest request body accepted. Bigger bodies get `413 Content Too Large`.
    pub max_body_size: usize,
    /// When this completes the server stops accepting connections, waits for
    /// in-flight requests to finish, and `serve` returns. `None` serves forever.
    pub shutdown: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            read_timeout: Duration::from_secs(30),
            max_body_size: DEFAULT_MAX_BODY_SIZE,
            shutdown: None,
        }
    }
}

impl Server {
    /// ```no_run
    /// use tokio::net::TcpListener;
    /// use yam_server::{Request, Response, Server, ServerConfig};
    ///
    /// # async fn run() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("localhost:3000").await?;
    /// let config = ServerConfig {
    ///     shutdown: Some(Box::pin(async { tokio::signal::ctrl_c().await.unwrap() })),
    ///     ..Default::default()
    /// };
    /// Server::serve(
    ///     listener,
    ///     |_request: Request| async { Response::new().send("Hello world") },
    ///     config,
    /// )
    /// .await
    /// # }
    /// ```
    pub async fn serve<H>(listener: TcpListener, handler: H, config: ServerConfig) -> io::Result<()>
    where
        H: Handler,
    {
        let handler: Arc<dyn Handler> = Arc::new(handler);
        let ServerConfig {
            read_timeout,
            max_body_size,
            shutdown,
        } = config;
        let mut shutdown = shutdown.unwrap_or_else(|| Box::pin(std::future::pending()));
        // Every connection holds a receiver; `closed()` resolves once all are dropped.
        let (stop, stopped) = watch::channel(false);
        loop {
            let (stream, _addr) = tokio::select! {
                accepted = listener.accept() => accepted?,
                _ = &mut shutdown => break,
            };
            // Disable Nagle so small responses aren't held back waiting on delayed ACKs.
            let _ = stream.set_nodelay(true);

            let handler = Arc::clone(&handler);
            let config = ServerConfig {
                read_timeout,
                max_body_size,
                shutdown: None,
            };
            let stopped = stopped.clone();

            tokio::spawn(async move { handle_request(stream, handler, config, stopped).await });
        }

        drop(listener);
        drop(stopped);
        let _ = stop.send(true);
        stop.closed().await;
        Ok(())
    }
}

/// Serves requests on one connection until the client closes it, it idles past
/// `config.read_timeout`, or `stopped` turns `true` while waiting for a request.
pub async fn handle_request(
    stream: TcpStream,
    handler: Arc<dyn Handler>,
    config: ServerConfig,
    mut stopped: watch::Receiver<bool>,
) -> Result<(), HttpError> {
    let remote_addr = stream.peer_addr().ok();
    let (read_stream, mut write_stream) = stream.into_split();

    let mut request_reader = RequestReader::new(read_stream).max_body_size(config.max_body_size);
    loop {
        let read = timeout(
            config.read_timeout,
            read_request(&mut request_reader, &mut write_stream),
        );
        let request = tokio::select! {
            read = read => read,
            _ = stopped.wait_for(|stopped| *stopped) => return Ok(()),
        };
        let mut request = match request {
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
        request.remote_addr = remote_addr;

        let mut connection_type = get_connection_type(&request);
        let is_head = request.method().eq_ignore_ascii_case("HEAD");

        let mut response = match handler.call(request).await {
            Ok(response) => response,
            Err(err) => err.into_response(),
        };
        if *stopped.borrow() {
            connection_type = ConnectionType::Close;
        }
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

/// Reads the head, answers `Expect: 100-continue` so the client sends the body, then reads the body.
async fn read_request<R, W>(reader: &mut RequestReader<R>, writer: &mut W) -> Result<Request, Error>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let request = reader.read_head().await?;
    if request.expects_continue() {
        writer.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await?;
    }
    reader.read_body(request).await
}
