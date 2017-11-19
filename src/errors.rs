use std::fmt;

use linky::LookupError;
use reqwest::StatusCode;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ErrorKind {
    HttpError,
    IoError,
    HttpStatus(StatusCode),
    NoDocument,
    NoFragment,
    Protocol,
    Absolute,
    InvalidUrl,
    NoMime,
    UnrecognizedMime,
    Prefixed,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ErrorKind::HttpError => write!(f, "HTTP_OTH"),
            ErrorKind::IoError => write!(f, "IO_ERR"),
            ErrorKind::InvalidUrl => write!(f, "URL_ERR"),
            ErrorKind::HttpStatus(status) => write!(f, "HTTP_{}", status.as_u16()),
            ErrorKind::NoDocument => write!(f, "NO_DOC"),
            ErrorKind::NoFragment => write!(f, "NO_FRAG"),
            ErrorKind::Protocol => write!(f, "PROTOCOL"),
            ErrorKind::Absolute => write!(f, "ABSOLUTE"),
            ErrorKind::NoMime => write!(f, "NO_MIME"),
            ErrorKind::UnrecognizedMime => write!(f, "MIME"),
            ErrorKind::Prefixed => write!(f, "PREFIXED"),
        }
    }
}

impl Into<LookupError> for ErrorKind {
    fn into(self) -> LookupError {
        LookupError {
            kind: self,
            cause: None,
        }
    }
}
