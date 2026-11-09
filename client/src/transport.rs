use std::sync::Arc;

use futures::StreamExt;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, pki_types::ServerName},
};
use url::Url;
use yam_shared::chunked::encode_chunk;

use crate::{error::Error, request::ChunkStream, response::Response};

/// Opens a connection for `url`, writes the request, and reads the response.
pub(crate) async fn send(
    url: &Url,
    tls_configuration: &Arc<ClientConfig>,
    request_head: &[u8],
    body: Vec<u8>,
    chunks: Option<ChunkStream>,
) -> Result<Response, Error> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::InvalidHostname(url.to_string()))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| Error::MissingPort(url.to_string()))?;

    match url.scheme() {
        "http" => {
            let mut stream = TcpStream::connect((host, port)).await?;
            write_and_read(&mut stream, request_head, body, chunks).await
        }
        "https" => {
            let server_name = ServerName::try_from(host.to_owned())
                .map_err(|_| Error::InvalidHostname(host.into()))?;
            let connector = TlsConnector::from(Arc::clone(tls_configuration));
            let mut stream = connector
                .connect(server_name, TcpStream::connect((host, port)).await?)
                .await?;
            write_and_read(&mut stream, request_head, body, chunks).await
        }
        scheme => Err(Error::UnsupportedScheme(scheme.into())),
    }
}

async fn write_and_read<S>(
    stream: &mut S,
    request_head: &[u8],
    body: Vec<u8>,
    chunks: Option<ChunkStream>,
) -> Result<Response, Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(request_head).await?;
    match chunks {
        Some(mut chunks) => {
            while let Some(chunk) = chunks.next().await {
                let chunk = chunk?;
                if !chunk.is_empty() {
                    stream.write_all(&encode_chunk(&chunk)).await?;
                }
            }
            stream.write_all(&encode_chunk(b"")).await?;
        }
        None => stream.write_all(&body).await?,
    }
    stream.flush().await?;

    Response::read(stream).await
}
