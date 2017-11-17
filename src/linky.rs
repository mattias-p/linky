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
use std::str::FromStr;

use bytecount::count;
use htmlstream;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
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
use url::ParseError;
use url::Url;
use url;

#[derive(Debug)]
pub enum LookupError {
    Client(reqwest::Error),
    Io(io::Error),
    HttpStatus(StatusCode),
    NoDocument,
    NoAnchor,
    Protocol,
    Absolute,
    Url(url::ParseError),
    Mime(Option<Mime>),
    Prefix,
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            LookupError::Client(_) => write!(f, "CLIENT"),
            LookupError::Io(_) => write!(f, "IO"),
            LookupError::HttpStatus(status) => write!(f, "HTTP_{}", status.as_u16()),
            LookupError::NoDocument => write!(f, "NO_DOCUMENT"),
            LookupError::NoAnchor => write!(f, "NO_ANCHOR"),
            LookupError::Protocol => write!(f, "PROTOCOL"),
            LookupError::Absolute => write!(f, "ABSOLUTE"),
            LookupError::Url(_) => write!(f, "URL"),
            LookupError::Mime(_) => write!(f, "MIME"),
            LookupError::Prefix => write!(f, "PREFIX"),
        }
    }
}

impl Error for LookupError {
    fn description(&self) -> &str {
        match *self {
            LookupError::Client(ref err) => err.description(),
            LookupError::Io(ref err) => err.description(),
            LookupError::HttpStatus(_) => "unexpected http status",
            LookupError::NoDocument => "document not found",
            LookupError::NoAnchor => "anchor not found",
            LookupError::Protocol => "unrecognized protocol",
            LookupError::Absolute => "unhandled absolute path",
            LookupError::Url(_) => "invalid url",
            LookupError::Mime(_) => "unrecognized mime type",
            LookupError::Prefix => "prefixed fragmendt",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            LookupError::Client(ref err) => Some(err),
            LookupError::Io(ref err) => Some(err),
            LookupError::Url(ref err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for LookupError {
    fn from(err: io::Error) -> Self {
        LookupError::Io(err)
    }
}

impl From<reqwest::Error> for LookupError {
    fn from(err: reqwest::Error) -> Self {
        LookupError::Client(err)
    }
}

impl From<url::ParseError> for LookupError {
    fn from(err: url::ParseError) -> Self {
        LookupError::Url(err)
    }
}

pub fn get_path_ids(path: &str, id_transform: &ToId) -> Result<Vec<String>, LookupError> {
    let mut headers = Headers::new();
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    Ok(
        MdAnchorParser::from_buffer(buffer.as_str(), id_transform, &mut headers)
            .map(|id| id.to_string())
            .collect(),
    )
}

pub fn get_url_ids(url: &Url, client: &Client) -> Result<Vec<String>, LookupError> {
    if url.scheme() == "http" || url.scheme() == "https" {
        let mut response = client.get(url.clone()).send()?;
        if !response.status().is_success() {
            Err(LookupError::HttpStatus(response.status()))?;
        }
        match response.headers().get::<ContentType>() {
            None => Err(LookupError::Mime(None))?,
            Some(&ContentType(ref mime_type))
                if mime_type.type_() != mime::TEXT || mime_type.subtype() != mime::HTML =>
            {
                Err(LookupError::Mime(Some(mime_type.clone())))?
            }
            _ => {}
        };
        let mut buffer = String::new();
        response.read_to_string(&mut buffer)?;
        Ok(get_html_ids(&buffer))
    } else {
        Err(LookupError::Protocol)
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
                Event::Start(Tag::Header(_)) => {
                    self.is_header = true;
                }
                Event::Text(text) => if self.is_header {
                    self.is_header = false;
                    let count = self.headers.register(text.to_string());
                    return Some(self.id_transform.to_id(text.as_ref(), count));
                },
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
    fn parse(s: &str) -> Result<Self, ParseError> {
        match Url::parse(s) {
            Ok(url) => Ok(Link::Url(url)),
            Err(ParseError::RelativeUrlWithoutBase) => Ok(Link::Path(s.to_string())),
            Err(err) => Err(err),
        }
    }

    pub fn split_fragment(&self) -> (Link, Option<String>) {
        match self {
            &Link::Path(ref path) => {
                let (path, fragment) = split_path_fragment(path);
                (
                    Link::Path(path.to_string()),
                    fragment.map(|f| f.to_string()),
                )
            }
            &Link::Url(ref url) => {
                let (url, fragment) = split_url_fragment(url);
                let mut url = url.clone();
                url.set_fragment(None);
                (Link::Url(url), fragment.map(|f| f.to_string()))
            }
        }
    }

    pub fn parse_with_root<P1: AsRef<Path>, P2: AsRef<Path>>(
        link: &str,
        origin: &P1,
        root: &P2,
    ) -> Result<Self, BaseLinkError> {
        match Url::parse(link) {
            Ok(url) => Ok(Link::Url(url)),
            Err(ParseError::RelativeUrlWithoutBase) => if Path::new(link).is_relative() {
                let link = if link.starts_with('#') {
                    let file_name = origin
                        .as_ref()
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
                Ok(Link::Path(
                    root.as_ref()
                        .join(as_relative(&link))
                        .to_string_lossy()
                        .to_string(),
                ))
            },
            Err(err) => Err(BaseLinkError::from(err)),
        }
    }
}

pub trait Targets {
    fn fetch_targets(&self, link: &Link) -> Result<Vec<String>, LookupError>;
}

impl Targets for Client {
    fn fetch_targets(&self, link: &Link) -> Result<Vec<String>, LookupError> {
        match link {
            &Link::Path(ref path) => if Path::new(path).is_relative() {
                get_path_ids(path.as_ref(), &GithubId)
            } else {
                Err(LookupError::Absolute)
            },
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
    ParseError(ParseError),
}

impl fmt::Display for BaseLinkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BaseLinkError::ParseError(ref err) => err.fmt(f),
            BaseLinkError::CannotBeABase => write!(f, "cannot be a base"),
        }
    }
}

impl From<ParseError> for BaseLinkError {
    fn from(err: ParseError) -> Self {
        BaseLinkError::ParseError(err)
    }
}

#[derive(Debug)]
pub struct BaseLink(Link);

impl FromStr for BaseLink {
    type Err = BaseLinkError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
            if let Event::Start(Tag::Link(url, _)) = event {
                self.linenum += count(
                    &self.buffer.as_bytes()[self.oldoffs..self.parser.get_offset()],
                    b'\n',
                );
                self.oldoffs = self.parser.get_offset();
                return Some((self.linenum, url));
            }
        }
        None
    }
}

pub fn md_file_links<'a>(
    path: &'a str,
    links: &mut Vec<(String, usize, String)>,
) -> io::Result<()> {
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    let parser = MdLinkParser::new(buffer.as_str()).map(|(lineno, url)| {
        (path.to_string(), lineno, url.as_ref().to_string())
    });

    links.extend(parser);
    Ok(())
}

pub struct LookupTag<'a>(pub Option<Option<&'a LookupError>>);

impl<'a> LookupTag<'a> {
    pub fn display(&self) -> Option<&fmt::Display> {
        match self.0 {
            Some(Some(ref err)) => Some(err as &fmt::Display),
            Some(None) => None,
            None => Some(&"" as &fmt::Display),
        }
    }
}
