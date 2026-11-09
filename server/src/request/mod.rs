mod chunked;
mod deserialization;
mod error;
mod method;
mod params;
mod request;
mod request_line;
mod request_reader;
pub mod request_ext;

// Re-export main types and functions
pub use error::{Error, ParamError, ParseError};
pub use method::{get_connection_type, ConnectionType, Method};
pub use request::Request;
pub use request_ext::Extensions;
pub use request_line::RequestLine;
pub use request_reader::RequestReader;