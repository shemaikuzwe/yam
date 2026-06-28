use std::{future::Future, io, pin::Pin, sync::Arc};

use tokio::net::{TcpListener, TcpStream};

use crate::{
    request::{Request, RequestReader},
    response::Response,
};

pub struct Server;

pub type HandlerFuture<'a> = Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;
pub trait Handler: Send + Sync + 'static {
    fn call<'a>(
        &'a self,
        req: &'a Request,
        res: &'a mut Response<&'a mut TcpStream>,
    ) -> HandlerFuture<'a>;
}
impl<F> Handler for F
where
    F: Send + Sync + 'static,
    F: for<'a> Fn(&'a Request, &'a mut Response<&'a mut TcpStream>) -> HandlerFuture<'a>,
{
    fn call<'a>(
        &'a self,
        req: &'a Request,
        res: &'a mut Response<&'a mut TcpStream>,
    ) -> HandlerFuture<'a> {
        self(req, res)
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
        request_reader.handle_request().await.unwrap()
    };

    let mut response = Response::new(&mut stream);

    handler.call(&request, &mut response).await?;

    Ok(())
}
