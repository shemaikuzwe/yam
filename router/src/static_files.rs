use std::{
    io,
    path::{Component, Path},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures::Stream;
use tokio::{fs::File, io::AsyncReadExt};
use yam_server::{HttpError, Request, Response, StatusCode};

/// Serves `root/{file}`, answering conditional requests with `304 Not Modified`.
pub async fn serve(root: &Path, req: Request) -> Result<Response, HttpError> {
    let file = Path::new(req.param("file").unwrap_or_default());
    // Only plain segments: blocks `..`, absolute paths and drive prefixes.
    if !file.components().all(|c| matches!(c, Component::Normal(_))) {
        return Ok(not_found());
    }
    let path = root.join(file);
    let meta = match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.is_file() => meta,
        Ok(_) => return Ok(not_found()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(not_found()),
        Err(err) => return Err(err.into()),
    };

    // HTTP dates have second precision; drop sub-seconds so comparisons match.
    let secs = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let modified = UNIX_EPOCH + Duration::from_secs(secs);
    let etag = format!("W/\"{:x}-{secs:x}\"", meta.len());
    let last_modified = httpdate::fmt_http_date(modified);

    let response = Response::new()
        .set("etag", &etag)
        .set("last-modified", &last_modified);
    if is_fresh(&req, &etag, modified) {
        return Ok(response.status(StatusCode::NotModified));
    }
    let file = tokio::fs::File::open(&path).await?;
    Ok(response
        .set("content-type", mime(&path))
        // A known length lets the writer skip chunked encoding.
        .set("content-length", &meta.len().to_string())
        .stream_response(read_chunks(file)))
}

fn read_chunks(file: File) -> impl Stream<Item = io::Result<Vec<u8>>> {
    futures::stream::try_unfold(file, |mut file| async move {
        let mut buf = vec![0; 64 * 1024];
        let n = file.read(&mut buf).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.truncate(n);
        Ok(Some((buf, file)))
    })
}

/// RFC 9110: `If-None-Match` wins over `If-Modified-Since` when both are sent.
fn is_fresh(req: &Request, etag: &str, modified: SystemTime) -> bool {
    if let Some(tags) = req.headers.get("if-none-match") {
        let etag = etag.trim_start_matches("W/");
        return tags
            .split(',')
            .map(|tag| tag.trim().trim_start_matches("W/"))
            .any(|tag| tag == "*" || tag == etag);
    }
    req.headers
        .get("if-modified-since")
        .and_then(|since| httpdate::parse_http_date(since).ok())
        .is_some_and(|since| modified <= since)
}

fn mime(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

fn not_found() -> Response {
    Response::new()
        .status(StatusCode::NotFound)
        .send("Not Found")
}
