use std::error;
use std::fmt;

use std::io;
use linky::FragmentPrefix;
use reqwest;
use reqwest::StatusCode;
use url;

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

#[derive(Debug)]
pub struct LookupError {
    pub kind: ErrorKind,
    pub cause: Option<Box<error::Error>>,
}

impl LookupError {
    pub fn kind(&self) -> ErrorKind {
        self.kind.clone()
    }

    pub fn from_prefix(prefix: String) -> Self {
        LookupError {
            kind: ErrorKind::Prefixed,
            cause: Some(Box::new(FragmentPrefix(prefix))),
        }
    }

    pub fn cause(&self) -> Option<&error::Error> {
        self.cause.as_ref().map(|e| e.as_ref())
    }
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::InvalidUrl => write!(f, "invalid url"),
            ErrorKind::HttpError => write!(f, "http error"),
            ErrorKind::IoError => write!(f, "io error"),
            ErrorKind::HttpStatus(status) => {
                write!(f,
                       "unexpected http status {}{}",
                       status.as_u16(),
                       status.canonical_reason()
                             .map(|s| format!(" {}", s))
                             .unwrap_or("".to_string()))
            }
            ErrorKind::NoDocument => write!(f, "document not found"),
            ErrorKind::NoFragment => write!(f, "fragment not found"),
            ErrorKind::Protocol => write!(f, "unhandled protocol"),
            ErrorKind::Absolute => write!(f, "unhandled absolute path"),
            ErrorKind::NoMime => write!(f, "no mime type"),
            ErrorKind::UnrecognizedMime => write!(f, "unrecognized mime type"),
            ErrorKind::Prefixed => write!(f, "prefixed fragment"),
        }
    }
}

impl error::Error for LookupError {
    fn description(&self) -> &str {
        match self.kind {
            ErrorKind::HttpError => "http error",
            ErrorKind::IoError => "io error",
            ErrorKind::InvalidUrl => "invalid url",
            ErrorKind::HttpStatus(_) => "unexpected http status",
            ErrorKind::NoDocument => "document not found",
            ErrorKind::NoFragment => "fragment not found",
            ErrorKind::Protocol => "unrecognized protocol",
            ErrorKind::Absolute => "unhandled absolute path",
            ErrorKind::NoMime => "no mime type",
            ErrorKind::UnrecognizedMime => "unrecognized mime type",
            ErrorKind::Prefixed => "prefixed fragmendt",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        self.cause.as_ref().map(|c| c.as_ref())
    }
}

impl From<io::Error> for LookupError {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::NotFound {
            LookupError {
                kind: ErrorKind::NoDocument,
                cause: Some(Box::new(err)),
            }
        } else {
            LookupError {
                kind: ErrorKind::IoError,
                cause: Some(Box::new(err)),
            }
        }
    }
}

impl From<reqwest::Error> for LookupError {
    fn from(err: reqwest::Error) -> Self {
        LookupError {
            kind: ErrorKind::HttpError,
            cause: Some(Box::new(err)),
        }
    }
}

impl From<url::ParseError> for LookupError {
    fn from(err: url::ParseError) -> Self {
        LookupError {
            kind: ErrorKind::InvalidUrl,
            cause: Some(Box::new(err)),
        }
    }
}
