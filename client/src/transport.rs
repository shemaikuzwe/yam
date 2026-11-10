use std::sync::Arc;

use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_rustls::rustls::ClientConfig;
use url::Url;
use yam_shared::chunked::encode_chunk;

use crate::{
    error::Error,
    pool::{Pools, Stream},
    request::{ChunkStream, Method},
    response::Response,
};

pub(crate) async fn send(
    pools: &Pools,
    url: &Url,
    tls_configuration: &Arc<ClientConfig>,
    method: Method,
    request_head: &[u8],
    body: Vec<u8>,
    chunks: Option<ChunkStream>,
) -> Result<Response, Error> {
    let mut conn = pools.get(url, tls_configuration).await?;

    // Marked reusable again only after a clean keep-alive response.
    conn.reusable = false;
    let (response, keep_alive) = match &mut conn.stream {
        Stream::Plain(stream) => write_and_read(stream, method, request_head, body, chunks).await?,
        Stream::Tls(stream) => write_and_read(stream, method, request_head, body, chunks).await?,
    };
    conn.reusable = keep_alive;
    Ok(response)
}

async fn write_and_read<S>(
    stream: &mut S,
    method: Method,
    request_head: &[u8],
    body: Vec<u8>,
    chunks: Option<ChunkStream>,
) -> Result<(Response, bool), Error>
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

    Response::read(stream, method).await
}
