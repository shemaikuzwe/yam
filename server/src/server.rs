use std::{future::Future, io, pin::Pin, sync::Arc};

use tokio::net::{TcpListener, TcpStream};

use crate::{
    request::{Request, RequestReader},
    response::{HttpError, IntoResponse, Response, ResponseWriter},
};

pub struct Server;

pub type HandlerFuture = Pin<Box<dyn Future<Output = Result<Response, HttpError>> + Send>>;
pub trait Handler: Send + Sync + 'static {
    fn call(&self, req: Request) -> HandlerFuture;
}
impl<F, Fut, R> Handler for F
where
    F: Send + Sync + 'static,
    F: Fn(Request) -> Fut,
    Fut: Future<Output = Result<R, HttpError>> + Send + 'static,
    R: crate::response::IntoResponse + Send + 'static,
{
    fn call(&self, req: Request) -> HandlerFuture {
        let fut = self(req);
        Box::pin(async move { fut.await.map(IntoResponse::into_response) })
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

            let handler = Arc::clone(&handler);

            tokio::spawn(async move { handle_request(stream, handler).await });
        }
    }
}

pub async fn handle_request(mut stream: TcpStream, handler: Arc<dyn Handler>) -> io::Result<()> {
    let request = {
        let mut request_reader = RequestReader::new(&mut stream);
        request_reader.handle_request().await
    };
    let request = match request {
        Ok(req) => req,
        Err(err) => {
            let response = HttpError::from(err).into_response();
            ResponseWriter::new(stream).send_response(response).await?;
            return Ok(());
        }
    };

    let response = match handler.call(request).await {
        Ok(response) => response,
        Err(err) => err.into_response(),
    };

    ResponseWriter::new(stream).send_response(response).await?;

    Ok(())
}
