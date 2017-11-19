use std::ascii::AsciiExt;
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::io;
use std::ops::Add;
use std::path::Path;
use std::result;
use std::str::FromStr;

use bytecount::count;
use htmlstream;
use pulldown_cmark;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use regex::Regex;
use reqwest::Client;
use reqwest::header::ContentType;
use reqwest::Method;
use reqwest::mime;
use reqwest::mime::Mime;
use reqwest::Response;
use reqwest::StatusCode;
use reqwest::header::Allow;
use reqwest;
use url::Url;
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

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct Tag(pub Result<(), ErrorKind>);

fn parse_status(s: &str) -> Result<Tag, ParseError> {
    if !s.starts_with("HTTP_") {
        return Err(ParseError);
    }
    u16::from_str(&s[5..]).ok()
        .and_then(|s| StatusCode::try_from(s).ok())
        .ok_or(ParseError)
        .map(|s| Tag(Err(ErrorKind::HttpStatus(s))))
}

impl FromStr for Tag {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "OK" => Ok(Tag(Ok(()))),
            "HTTP_OTH" => Ok(Tag(Err(ErrorKind::HttpError))),
            "IO_ERR" => Ok(Tag(Err(ErrorKind::IoError))),
            "URL_ERR" => Ok(Tag(Err(ErrorKind::InvalidUrl))),
            "NO_DOC" => Ok(Tag(Err(ErrorKind::NoDocument))),
            "NO_FRAG" => Ok(Tag(Err(ErrorKind::NoFragment))),
            "PROTOCOL" => Ok(Tag(Err(ErrorKind::Protocol))),
            "ABSOLUTE" => Ok(Tag(Err(ErrorKind::Absolute))),
            "NO_MIME" => Ok(Tag(Err(ErrorKind::NoMime))),
            "MIME" => Ok(Tag(Err(ErrorKind::UnrecognizedMime))),
            "PREFIXED" => Ok(Tag(Err(ErrorKind::Prefixed))),
            s => parse_status(&s),
        }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0 {
            &Ok(()) => write!(f, "OK"),
            &Err(ref kind) => write!(f, "{}", kind),
        }
    }
}

#[derive(Debug)]
pub struct ParseError;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid tag")
    }
}

impl Error for ParseError {
    fn description(&self) -> &str {
        &"invalid tag"
    }
    fn cause(&self) -> Option<&Error> {
        None
    }
}

#[derive(Debug)]
pub struct LookupError {
    kind: ErrorKind,
    cause: Option<Box<Error>>,
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

    pub fn cause(&self) -> Option<&Error> {
        self.cause.as_ref().map(|e| e.as_ref())
    }
}

#[derive(Debug)]
pub struct UnrecognizedMime(Mime);

impl fmt::Display for UnrecognizedMime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "unrecognized mime type {}", self.0)
    }
}

impl Error for UnrecognizedMime {
    fn description(&self) -> &str {
        "unrecognied mime type"
    }
    fn cause(&self) -> Option<&Error> {
        None
    }
}

#[derive(Debug)]
pub struct FragmentPrefix(String);

impl fmt::Display for FragmentPrefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "fragment prefix: {}", self.0)
    }
}

impl Error for FragmentPrefix {
    fn description(&self) -> &str {
        "prefixed fragment"
    }
    fn cause(&self) -> Option<&Error> {
        None
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

impl Error for LookupError {
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

    fn cause(&self) -> Option<&Error> {
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

pub fn get_path_ids(path: &str, id_transform: &ToId) -> result::Result<Vec<String>, LookupError> {
    let mut headers = Headers::new();
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    Ok(MdAnchorParser::from_buffer(buffer.as_str(), id_transform, &mut headers)
           .map(|id| id.to_string())
           .collect())
}

pub fn get_url_ids(url: &Url, client: &Client) -> result::Result<Vec<String>, LookupError> {
    if url.scheme() == "http" || url.scheme() == "https" {
        let mut response = client.get(url.clone()).send()?;
        if !response.status().is_success() {
            return Err(ErrorKind::HttpStatus(response.status()).into());
        }
        match response.headers().get::<ContentType>() {
            None => return Err(ErrorKind::NoMime.into()),
            Some(&ContentType(ref mime_type)) if mime_type.type_() != mime::TEXT ||
                                                 mime_type.subtype() != mime::HTML => {
                return Err(LookupError {
                    kind: ErrorKind::UnrecognizedMime,
                    cause: Some(Box::new(UnrecognizedMime(mime_type.clone()))),
                })
            }
            _ => {}
        };
        let mut buffer = String::new();
        response.read_to_string(&mut buffer)?;
        Ok(get_html_ids(&buffer))
    } else {
        Err(ErrorKind::Protocol.into())
    }
}

fn get_html_ids(buffer: &str) -> Vec<String> {
    let mut result = vec![];
    for (_, tag) in htmlstream::tag_iter(buffer) {
        for (_, attr) in htmlstream::attr_iter(&tag.attributes) {
            if attr.name == "id" || (tag.name == "a" && attr.name == "name") {
                result.push(attr.value.clone());
            }
        }
    }
    result
}

trait AllowsMethod {
    fn allows_method(&self, method: Method) -> bool;
}

impl AllowsMethod for Response {
    fn allows_method(&self, method: Method) -> bool {
        self.headers()
            .get::<Allow>()
            .map_or(false, |allow| allow.0.iter().any(|m| *m == method))
    }
}

fn as_relative<'a, P: AsRef<Path>>(path: &'a P) -> &'a Path {
    let mut components = path.as_ref().components();
    while components.as_path().has_root() {
        components.next();
    }
    components.as_path()
}

pub fn split_fragment(path: &str) -> Option<(&str, &str)> {
    if let Some(pos) = path.find('#') {
        Some((&path[0..pos], &path[pos + 1..]))
    } else {
        None
    }
}

fn split_path_fragment(path: &str) -> (&str, Option<&str>) {
    if let Some((path, fragment)) = split_fragment(path) {
        (path, Some(fragment))
    } else {
        (path, None)
    }
}

fn split_url_fragment(url: &Url) -> (&Url, Option<&str>) {
    (url, url.fragment())
}

struct MdAnchorParser<'a> {
    parser: Parser<'a>,
    is_header: bool,
    headers: &'a mut Headers,
    id_transform: &'a ToId,
}

impl<'a> MdAnchorParser<'a> {
    fn new(parser: Parser<'a>, id_transform: &'a ToId, headers: &'a mut Headers) -> Self {
        MdAnchorParser {
            parser: parser,
            is_header: false,
            headers: headers,
            id_transform: id_transform,
        }
    }

    fn from_buffer(buffer: &'a str, id_transform: &'a ToId, headers: &'a mut Headers) -> Self {
        MdAnchorParser::new(Parser::new(buffer), id_transform, headers)
    }
}

impl<'a> Iterator for MdAnchorParser<'a> {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.parser.next() {
            match event {
                Event::Start(pulldown_cmark::Tag::Header(_)) => {
                    self.is_header = true;
                }
                Event::Text(text) => {
                    if self.is_header {
                        self.is_header = false;
                        let count = self.headers.register(text.to_string());
                        return Some(self.id_transform.to_id(text.as_ref(), count));
                    }
                }
                _ => (),
            }
        }
        None
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Link {
    Url(Url),
    Path(String),
}

impl Link {
    fn parse(s: &str) -> result::Result<Self, url::ParseError> {
        match Url::parse(s) {
            Ok(url) => Ok(Link::Url(url)),
            Err(url::ParseError::RelativeUrlWithoutBase) => Ok(Link::Path(s.to_string())),
            Err(err) => Err(err),
        }
    }

    pub fn split_fragment(&self) -> (Link, Option<String>) {
        match self {
            &Link::Path(ref path) => {
                let (path, fragment) = split_path_fragment(path);
                (Link::Path(path.to_string()),
                 fragment.map(|f| f.to_string()))
            }
            &Link::Url(ref url) => {
                let (url, fragment) = split_url_fragment(url);
                let mut url = url.clone();
                url.set_fragment(None);
                (Link::Url(url), fragment.map(|f| f.to_string()))
            }
        }
    }

    pub fn parse_with_root<P1: AsRef<Path>, P2: AsRef<Path>>(link: &str,
                                                             origin: &P1,
                                                             root: &P2)
                                                             -> result::Result<Self, BaseLinkError> {
        match Url::parse(link) {
            Ok(url) => Ok(Link::Url(url)),
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                if Path::new(link).is_relative() {
                    let link = if link.starts_with('#') {
                        let file_name = origin.as_ref()
                                              .file_name()
                                              .unwrap()
                                              .to_string_lossy()
                                              .to_string()
                                              .add(link);
                        origin.as_ref().with_file_name(file_name)
                    } else {
                        origin.as_ref().with_file_name(link)
                    };
                    Ok(Link::Path(link.to_string_lossy().to_string()))
                } else {
                    Ok(Link::Path(root.as_ref()
                                      .join(as_relative(&link))
                                      .to_string_lossy()
                                      .to_string()))
                }
            }
            Err(err) => Err(BaseLinkError::from(err)),
        }
    }
}

pub trait Targets {
    fn fetch_targets(&self, link: &Link) -> result::Result<Vec<String>, LookupError>;
}

impl Targets for Client {
    fn fetch_targets(&self, link: &Link) -> result::Result<Vec<String>, LookupError> {
        match link {
            &Link::Path(ref path) => {
                if Path::new(path).is_relative() {
                    get_path_ids(path.as_ref(), &GithubId)
                } else {
                    Err(ErrorKind::Absolute.into())
                }
            }
            &Link::Url(ref url) => get_url_ids(url, self),
        }
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Link::Url(ref url) => write!(f, "{}", url),
            Link::Path(ref path) => write!(f, "{}", path),
        }
    }
}
impl Error for BaseLinkError {
    fn description(&self) -> &str {
        match *self {
            BaseLinkError::ParseError(ref err) => err.description(),
            BaseLinkError::CannotBeABase => "cannot be a base",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            BaseLinkError::ParseError(ref err) => Some(err),
            BaseLinkError::CannotBeABase => None,
        }
    }
}

#[derive(Debug)]
pub enum BaseLinkError {
    CannotBeABase,
    ParseError(url::ParseError),
}

impl fmt::Display for BaseLinkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BaseLinkError::ParseError(ref err) => err.fmt(f),
            BaseLinkError::CannotBeABase => write!(f, "cannot be a base"),
        }
    }
}

impl From<url::ParseError> for BaseLinkError {
    fn from(err: url::ParseError) -> Self {
        BaseLinkError::ParseError(err)
    }
}

#[derive(Debug)]
pub struct BaseLink(Link);

impl FromStr for BaseLink {
    type Err = BaseLinkError;
    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        match Link::parse(s) {
            Ok(Link::Url(ref base)) if base.cannot_be_a_base() => Err(BaseLinkError::CannotBeABase),
            Ok(link) => Ok(BaseLink(link)),
            Err(err) => Err(BaseLinkError::ParseError(err)),
        }
    }
}


fn slurp<P: AsRef<Path>>(filename: &P, mut buffer: &mut String) -> io::Result<usize> {
    File::open(filename.as_ref())?.read_to_string(&mut buffer)
}

lazy_static! {
    static ref GITHUB_PUNCTUATION: Regex = Regex::new(r"[^\w -]").unwrap();
}

pub trait ToId {
    fn to_id(&self, text: &str, repetition: usize) -> String;
}

pub struct GithubId;

impl ToId for GithubId {
    fn to_id(&self, text: &str, repetition: usize) -> String {
        let text = GITHUB_PUNCTUATION.replace_all(text, "");
        let text = text.to_ascii_lowercase();
        let text = text.replace('-', "-");
        if repetition == 0 {
            text
        } else {
            format!("{}-{}", text, repetition)
        }
    }
}

pub struct Headers(HashMap<String, usize>);

impl Headers {
    pub fn new() -> Self {
        Headers(HashMap::new())
    }

    pub fn register(&mut self, text: String) -> usize {
        match self.0.entry(text.to_string()) {
            Entry::Occupied(ref mut occupied) => {
                let count = *occupied.get();
                *occupied.get_mut() = count + 1;
                count
            }
            Entry::Vacant(vacant) => {
                vacant.insert(1);
                0
            }
        }
    }
}

struct MdLinkParser<'a> {
    buffer: &'a str,
    parser: Parser<'a>,
    linenum: usize,
    oldoffs: usize,
}

impl<'a> MdLinkParser<'a> {
    fn new(buffer: &'a str) -> Self {
        MdLinkParser {
            parser: Parser::new(buffer),
            buffer: buffer,
            linenum: 1,
            oldoffs: 0,
        }
    }
}

impl<'a> Iterator for MdLinkParser<'a> {
    type Item = (usize, Cow<'a, str>);
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.parser.next() {
            if let Event::Start(pulldown_cmark::Tag::Link(url, _)) = event {
                self.linenum += count(&self.buffer.as_bytes()[self.oldoffs..self.parser
                                                                                .get_offset()],
                                      b'\n');
                self.oldoffs = self.parser.get_offset();
                return Some((self.linenum, url));
            }
        }
        None
    }
}

pub fn md_file_links<'a>(path: &'a str,
                         links: &mut Vec<(String, usize, String)>)
                         -> io::Result<()> {
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    let parser = MdLinkParser::new(buffer.as_str())
                     .map(|(lineno, url)| (path.to_string(), lineno, url.as_ref().to_string()));

    links.extend(parser);
    Ok(())
}

pub enum BorrowedOrOwned<'a, T: 'a> {
    Borrowed(&'a T),
    Owned(T),
}

impl<'a, T> BorrowedOrOwned<'a, T> {
    pub fn as_ref(&self) -> &T {
        use self::BorrowedOrOwned::*;

        match self {
            &Borrowed(b) => b,
            &Owned(ref o) => o,
        }
    }
}

pub fn find_prefixed_fragment(ids: &Vec<String>, fragment: &String, prefixes: &Vec<String>) -> Option<String> {
    prefixes
        .iter()
        .filter_map(|p| {
            if ids.contains(&format!("{}{}", p, fragment)) {
                Some(p.to_string())
            } else {
                None
            }
        })
        .next()
}

