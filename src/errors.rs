use std::error;
use std::fmt;
use std::io;
use std::str::FromStr;

use linky::Link;
use reqwest;
use reqwest::mime::Mime;
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

impl ErrorKind {
    fn from_http_status_str(s: &str) -> Result<ErrorKind, ParseError> {
        if !s.starts_with("HTTP_") {
            return Err(ParseError);
        }
        u16::from_str(&s[5..])
            .ok()
            .and_then(|s| StatusCode::try_from(s).ok())
            .map(ErrorKind::HttpStatus)
            .ok_or(ParseError)
    }
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

impl FromStr for ErrorKind {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "HTTP_OTH" => Ok(ErrorKind::HttpError),
            "IO_ERR" => Ok(ErrorKind::IoError),
            "URL_ERR" => Ok(ErrorKind::InvalidUrl),
            "NO_DOC" => Ok(ErrorKind::NoDocument),
            "NO_FRAG" => Ok(ErrorKind::NoFragment),
            "PROTOCOL" => Ok(ErrorKind::Protocol),
            "ABSOLUTE" => Ok(ErrorKind::Absolute),
            "NO_MIME" => Ok(ErrorKind::NoMime),
            "MIME" => Ok(ErrorKind::UnrecognizedMime),
            "PREFIXED" => Ok(ErrorKind::Prefixed),
            s => ErrorKind::from_http_status_str(s),
        }
    }
}

#[derive(Debug)]
pub struct ParseError;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid tag")
    }
}

impl error::Error for ParseError {
    fn description(&self) -> &str {
        "invalid tag"
    }
    fn cause(&self) -> Option<&error::Error> {
        None
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

    #[allow(dead_code)]
    pub fn cause(&self) -> Option<&error::Error> {
        self.cause.as_ref().map(|e| e.as_ref())
    }
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::InvalidUrl => write!(f, "Invalid url"),
            ErrorKind::HttpError => write!(f, "HTTP error"),
            ErrorKind::IoError => write!(f, "IO error"),
            ErrorKind::HttpStatus(status) => write!(
                f,
                "Unexpected HTTP status {}{}",
                status.as_u16(),
                status
                    .canonical_reason()
                    .map(|s| format!(" {}", s))
                    .unwrap_or_else(String::new)
            ),
            ErrorKind::NoDocument => write!(f, "Document not found"),
            ErrorKind::NoFragment => write!(f, "Fragment not found"),
            ErrorKind::Protocol => write!(f, "Unhandled protocol"),
            ErrorKind::Absolute => write!(f, "Unable to handle absolute path"),
            ErrorKind::NoMime => write!(f, "No mime type"),
            ErrorKind::UnrecognizedMime => write!(f, "Unrecognized mime type"),
            ErrorKind::Prefixed => write!(f, "Fragment not found without prefix"),
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

#[derive(Debug)]
pub struct UnrecognizedMime(Mime);

impl UnrecognizedMime {
    pub fn new(mime: Mime) -> Self {
        UnrecognizedMime(mime)
    }
}

impl fmt::Display for UnrecognizedMime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Unrecognized mime type {}", self.0)
    }
}

impl error::Error for UnrecognizedMime {
    fn description(&self) -> &str {
        "unrecognied mime type"
    }

    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

#[derive(Debug)]
pub struct PrefixError {
    prefix: String,
    cause: Box<error::Error>,
}

impl PrefixError {
    pub fn new(prefix: String, cause: Box<error::Error>) -> Self {
        PrefixError {
            prefix: prefix,
            cause: cause,
        }
    }
}

impl fmt::Display for PrefixError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Prefix: {}", self.prefix)
    }
}

impl error::Error for PrefixError {
    fn description(&self) -> &str {
        "prefixed fragment"
    }

    fn cause(&self) -> Option<&error::Error> {
        Some(&*self.cause)
    }
}

#[derive(Debug)]
pub struct LinkError {
    link: Link,
    cause: Box<error::Error>,
}

impl LinkError {
    pub fn new(link: Link, cause: Box<error::Error>) -> Self {
        LinkError {
            link: link,
            cause: cause,
        }
    }
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Link: {}", self.link)
    }
}

impl error::Error for LinkError {
    fn description(&self) -> &str {
        "link error"
    }

    fn cause(&self) -> Option<&error::Error> {
        Some(&*self.cause)
    }
}

#[derive(Debug)]
pub struct FragmentError {
    fragment: String,
    cause: Box<error::Error>,
}

impl FragmentError {
    pub fn new(fragment: String, cause: Box<error::Error>) -> Self {
        FragmentError {
            fragment: fragment,
            cause: cause,
        }
    }
}

impl fmt::Display for FragmentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Fragment: {}", self.fragment)
    }
}

impl error::Error for FragmentError {
    fn description(&self) -> &str {
        "fragment error"
    }

    fn cause(&self) -> Option<&error::Error> {
        Some(&*self.cause)
    }
}
