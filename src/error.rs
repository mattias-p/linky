use std::borrow::Cow;
use std::error;
use std::fmt;
use std::io;
use std::iter::Iterator;
use std::mem;
use std::result;
use std::str::FromStr;

use reqwest::StatusCode;

pub type Result<T> = result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Tag {
    Ok,
    HttpError,
    Timeout,
    IoError,
    HttpStatus(StatusCode),
    NoDocument,
    NoFragment,
    Protocol,
    Absolute,
    Directory,
    InvalidUrl,
    NoMime,
    UnrecognizedMime,
    DecodingError,
    Prefixed,
    CaseInsensitiveFragment,
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Tag::Ok => write!(f, "OK"),
            Tag::HttpError => write!(f, "HTTP_OTH"),
            Tag::Timeout => write!(f, "TIMEOUT"),
            Tag::IoError => write!(f, "IO_ERR"),
            Tag::InvalidUrl => write!(f, "URL_ERR"),
            Tag::HttpStatus(status) => write!(f, "HTTP_{}", status.as_u16()),
            Tag::NoDocument => write!(f, "NO_DOC"),
            Tag::NoFragment => write!(f, "NO_FRAG"),
            Tag::Protocol => write!(f, "PROTOCOL"),
            Tag::Absolute => write!(f, "ABSOLUTE"),
            Tag::Directory => write!(f, "DIR"),
            Tag::NoMime => write!(f, "NO_MIME"),
            Tag::UnrecognizedMime => write!(f, "MIME"),
            Tag::DecodingError => write!(f, "DEC_ERR"),
            Tag::Prefixed => write!(f, "PREFIXED"),
            Tag::CaseInsensitiveFragment => write!(f, "CASE_FRAG"),
        }
    }
}

impl FromStr for Tag {
    type Err = MsgError;
    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "OK" => Ok(Tag::Ok),
            "HTTP_OTH" => Ok(Tag::HttpError),
            "TIMEOUT" => Ok(Tag::Timeout),
            "IO_ERR" => Ok(Tag::IoError),
            "URL_ERR" => Ok(Tag::InvalidUrl),
            "NO_DOC" => Ok(Tag::NoDocument),
            "NO_FRAG" => Ok(Tag::NoFragment),
            "PROTOCOL" => Ok(Tag::Protocol),
            "ABSOLUTE" => Ok(Tag::Absolute),
            "DIR" => Ok(Tag::Directory),
            "NO_MIME" => Ok(Tag::NoMime),
            "MIME" => Ok(Tag::UnrecognizedMime),
            "PREFIXED" => Ok(Tag::Prefixed),
            "CASE_FRAG" => Ok(Tag::CaseInsensitiveFragment),
            s if s.starts_with("HTTP_") => u16::from_str(&s[5..])
                .ok()
                .and_then(|s| StatusCode::from_u16(s).ok())
                .map(Tag::HttpStatus)
                .ok_or_else(|| MsgError(Cow::from("Invalid tag"))),
            _ => Err(MsgError(Cow::from("Invalid tag"))),
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub tag: Tag,
    msgs: Vec<Cow<'static, str>>,
    cause: Option<Box<dyn error::Error + Sync + Send + 'static>>,
}

impl Tag {
    pub fn as_error(&self) -> Error {
        Error {
            tag: *self,
            msgs: vec![],
            cause: None,
        }
    }
}

impl Error {
    pub fn context(mut self, msg: Cow<'static, str>) -> Self {
        self.msgs.push(msg);
        self
    }

    #[allow(dead_code)]
    pub fn cause(&self) -> Option<&(dyn error::Error + Sync + Send)> {
        self.cause.as_ref().map(|e| e.as_ref())
    }

    pub fn decoding_error(msg: Cow<'static, str>) -> Self {
        Error {
            tag: Tag::DecodingError,
            msgs: vec![],
            cause: Some(Box::new(MsgError(msg))),
        }
    }

    pub fn iter(&self) -> ErrorIter {
        ErrorIter {
            count: 0,
            err: self,
            cause: self.cause().map(|c| c as &dyn error::Error),
        }
    }
}

pub struct ErrorIter<'a> {
    count: usize,
    err: &'a Error,
    cause: Option<&'a dyn error::Error>,
}

impl<'a> Iterator for ErrorIter<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            self.count += 1;
            Some(format!("{}", self.err))
        } else if self.count <= self.err.msgs.len() {
            let elem = &self.err.msgs[self.err.msgs.len() - self.count];
            self.count += 1;
            Some(format!("  context: {elem}"))
        } else if let Some(cause) = mem::replace(&mut self.cause, None) {
            let s = format!("  caused by: {cause}");
            self.cause = cause.source();
            Some(s)
        } else {
            None
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.tag {
            Tag::Ok => write!(f, "Ok"),
            Tag::InvalidUrl => write!(f, "Invalid url"),
            Tag::HttpError => write!(f, "HTTP error"),
            Tag::Timeout => write!(f, "Timeout"),
            Tag::IoError => write!(f, "IO error"),
            Tag::HttpStatus(status) => write!(
                f,
                "Unexpected HTTP status {}{}",
                status.as_u16(),
                status
                    .canonical_reason()
                    .map(|s| format!(" {s}"))
                    .unwrap_or_else(String::new)
            ),
            Tag::NoDocument => write!(f, "Document not found"),
            Tag::NoFragment => write!(f, "Fragment not found"),
            Tag::Protocol => write!(f, "Unhandled protocol"),
            Tag::Absolute => write!(f, "Unable to handle absolute path"),
            Tag::Directory => write!(f, "Document is a directory"),
            Tag::NoMime => write!(f, "No mime type"),
            Tag::UnrecognizedMime => write!(f, "Unrecognized mime type"),
            Tag::DecodingError => write!(f, "Decoding error"),
            Tag::Prefixed => write!(f, "Fragment not found without prefix"),
            Tag::CaseInsensitiveFragment => write!(f, "Fragment not found case-sensitively"),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match self.tag {
            Tag::Ok => "ok",
            Tag::HttpError => "http error",
            Tag::Timeout => "timeout",
            Tag::IoError => "io error",
            Tag::InvalidUrl => "invalid url",
            Tag::HttpStatus(_) => "unexpected http status",
            Tag::NoDocument => "document not found",
            Tag::NoFragment => "fragment not found",
            Tag::Protocol => "unrecognized protocol",
            Tag::Absolute => "unhandled absolute path",
            Tag::Directory => "document is a directory",
            Tag::NoMime => "no mime type",
            Tag::UnrecognizedMime => "unrecognized mime type",
            Tag::DecodingError => "decoding error",
            Tag::Prefixed => "prefixed fragmendt",
            Tag::CaseInsensitiveFragment => "case-insensitive fragmendt",
        }
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        self.cause.as_ref().map(|c| c.as_ref() as &dyn error::Error)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::NotFound {
            Error {
                tag: Tag::NoDocument,
                msgs: vec![],
                cause: Some(Box::new(err)),
            }
        } else {
            Error {
                tag: Tag::IoError,
                msgs: vec![],
                cause: Some(Box::new(err)),
            }
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Error {
                tag: Tag::Timeout,
                msgs: vec![],
                cause: Some(Box::new(err)),
            }
        } else {
            Error {
                tag: Tag::HttpError,
                msgs: vec![],
                cause: Some(Box::new(err)),
            }
        }
    }
}

impl From<mime::FromStrError> for Error {
    fn from(err: mime::FromStrError) -> Self {
        Error {
            tag: Tag::UnrecognizedMime,
            msgs: vec![],
            cause: Some(Box::new(err)),
        }
    }
}

impl From<reqwest::header::ToStrError> for Error {
    fn from(err: reqwest::header::ToStrError) -> Self {
        Error {
            tag: Tag::HttpError,
            msgs: vec![],
            cause: Some(Box::new(err)),
        }
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Error {
            tag: Tag::InvalidUrl,
            msgs: vec![],
            cause: Some(Box::new(err)),
        }
    }
}

#[derive(Debug)]
pub struct MsgError(pub Cow<'static, str>);

impl fmt::Display for MsgError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl error::Error for MsgError {
    fn description(&self) -> &str {
        &self.0
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}
