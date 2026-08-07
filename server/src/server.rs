use std::{future::Future, io, pin::Pin, sync::Arc};

use tokio::net::{TcpListener, TcpStream};

use crate::{
    request::{Request, RequestReader},
    response::{Response, StatusCode},
};

pub struct Server;

pub type HandlerFuture = Pin<Box<dyn Future<Output = io::Result<()>> + Send>>;
pub trait Handler: Send + Sync + 'static {
    fn call(&self, req: Request, res: Response<TcpStream>) -> HandlerFuture;
}
impl<F, Fut> Handler for F
where
    F: Send + Sync + 'static,
    F: Fn(Request, Response<TcpStream>) -> Fut,
    Fut: Future<Output = io::Result<()>> + Send + 'static,
{
    fn call(&self, req: Request, res: Response<TcpStream>) -> HandlerFuture {
        Box::pin(self(req, res))
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

            tokio::spawn(async move {
                if let Err(err) = handle_request(stream, handler).await {
                    eprintln!("Connection failed: {err}");
                }
            });
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
        Err(_) => {
            let res = Response::new(stream);
            res.status(StatusCode::StatusBadRequest)
                .send("Bad request")
                .await?;
            return Ok(());
        }
    };

    let response = Response::new(stream);

    handler.call(request, response).await?;

    Ok(())
}
