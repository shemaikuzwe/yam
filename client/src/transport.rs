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

/// Bodies up to this size are copied after the head so the request goes out in
/// one write; larger ones are written separately to avoid the copy.
const INLINE_BODY_LIMIT: usize = 64 * 1024;

pub(crate) async fn send(
    pools: &Pools,
    url: &Url,
    tls_configuration: &Arc<ClientConfig>,
    method: Method,
    request_head: Vec<u8>,
    body: &[u8],
    chunks: Option<ChunkStream>,
) -> Result<Response, Error> {
    let mut conn = pools.get(url, tls_configuration).await?;

    // Marked reusable again only after a clean keep-alive response.
    conn.reusable = false;
    let conn = &mut *conn;
    let (response, keep_alive) = match &mut conn.stream {
        Stream::Plain(stream) => {
            write_and_read(stream, &mut conn.buf, method, request_head, body, chunks).await?
        }
        Stream::Tls(stream) => {
            write_and_read(stream, &mut conn.buf, method, request_head, body, chunks).await?
        }
    };
    conn.reusable = keep_alive;
    Ok(response)
}

async fn write_and_read<S>(
    stream: &mut S,
    buf: &mut Vec<u8>,
    method: Method,
    mut request: Vec<u8>,
    body: &[u8],
    chunks: Option<ChunkStream>,
) -> Result<(Response, bool), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match chunks {
        Some(mut chunks) => {
            stream.write_all(&request).await?;
            while let Some(chunk) = chunks.next().await {
                let chunk = chunk?;
                if !chunk.is_empty() {
                    stream.write_all(&encode_chunk(&chunk)).await?;
                }
            }
            stream.write_all(&encode_chunk(b"")).await?;
        }
        None if body.len() <= INLINE_BODY_LIMIT => {
            request.extend_from_slice(body);
            stream.write_all(&request).await?;
        }
        None => {
            stream.write_all(&request).await?;
            stream.write_all(body).await?;
        }
    }
    stream.flush().await?;

    buf.clear();
    Response::read(stream, buf, method).await
}
