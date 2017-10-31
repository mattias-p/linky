extern crate bytecount;
extern crate htmlstream;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate shell_escape;
extern crate structopt;
extern crate unicode_categories;
extern crate unicode_normalization;
extern crate url;

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::io;
use std::ops::Add;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use bytecount::count;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use reqwest::Client;
use reqwest::StatusCode;
use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;
use url::ParseError;
use url::Url;

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

pub fn check_skippable<'a>(link: &Link, client: &Client) -> Result<(), LookupError> {
    match *link {
        Link::Path(ref path) => {
            if Path::new(path).is_relative() {
                check_skippable_path(path.as_ref())
            } else {
                Err(LookupError::Absolute)
            }
        },
        Link::Url(ref url) => check_skippable_url(url, client),
    }
}

fn check_skippable_path(path: &str) -> Result<(), LookupError> {
    if let Some((path, fragment)) = split_fragment(path) {
        let mut buffer = String::new();
        slurp(&path, &mut buffer)?;
        if MdAnchorParser::from(buffer.as_str()).any(|anchor| *anchor == *fragment) {
            Ok(())
        } else {
            Err(LookupError::NoAnchor)
        }
    } else {
        if Path::new(path).exists() {
            Ok(())
        } else {
            Err(LookupError::NoDocument)
        }
    }
}

fn check_skippable_url(url: &Url, client: &Client) -> Result<(), LookupError> {
    if url.scheme() == "http" || url.scheme() == "https" {
        if let Some(fragment) = url.fragment() {
            let mut response = client.get(url.clone()).send()?;
            if !response.status().is_success() {
                Err(LookupError::HttpStatus(response.status()))?;
            }
            let mut buffer = String::new();
            response.read_to_string(&mut buffer)?;
            if has_html_anchor(&buffer, fragment) {
                Ok(())
            } else {
                Err(LookupError::NoAnchor)
            }
        } else {
            let response = client.head(url.clone()).send()?;
            if response.status().is_success() {
                Ok(())
            } else {
                Err(LookupError::HttpStatus(response.status()))
            }
        }
    } else {
        Err(LookupError::Protocol)
    }
}

fn as_relative<'a, P: AsRef<Path>>(path: &'a P) -> &'a Path {
    let mut components = path.as_ref().components();
    while components.as_path().has_root() {
        components.next();
    }
    components.as_path()
}

fn join_absolute<P1: AsRef<Path>, P2: AsRef<Path>>(base_path: &P1, path: &P2) -> PathBuf {
    base_path.as_ref().join(as_relative(path))
}

fn has_html_anchor(buffer: &str, anchor: &str) -> bool {
    for (_, tag) in htmlstream::tag_iter(buffer) {
        for (_, attr) in htmlstream::attr_iter(&tag.attributes) {
            if tag.name == "a" && (attr.name == "name" || attr.name == "id") {
                if attr.value == anchor {
                    return true;
                }
            }
        }
    }
    return false;
}

fn split_fragment(path: &str) -> Option<(&str, &str)> {
    if let Some(pos) = path.find('#') {
        Some((&path[0..pos], &path[pos+1..]))
    } else {
        None
    }
}

struct MdAnchorParser<'a> {
    parser: Parser<'a>,
    is_header: bool,
}

impl<'a> MdAnchorParser<'a> {
    fn new(parser: Parser<'a>) -> Self {
        MdAnchorParser {
            parser: parser,
            is_header: false,
        }
    }
}

impl<'a> From<&'a str> for MdAnchorParser<'a> {
    fn from(buffer: &'a str) -> Self {
        MdAnchorParser::new(Parser::new(buffer))
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
                    return Some(anchor(text.as_ref()));
                },
                _ => (),
            }
        }
        None
    }
}

#[derive(Debug)]
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

    pub fn parse_with_origin(link: &str, origin: &Path) -> Result<Self, BaseLinkError> {
        match Url::parse(link) {
            Ok(url) => {
                Ok(Link::Url(url))
            }
            Err(ParseError::RelativeUrlWithoutBase) => {
                if Path::new(link).is_relative() {
                    let link = if link.starts_with('#') {
                        let file_name = origin.file_name().unwrap().to_string_lossy().to_string().add(link);
                        origin.with_file_name(file_name)
                    } else {
                        origin.with_file_name(link)
                    };
                    Ok(Link::Path(link.to_string_lossy().to_string()))
                } else {
                    Ok(Link::Path(link.to_string()))
                }
            }
            Err(err) => Err(BaseLinkError::from(err)),
        }
    }

    pub fn parse_with_base(link: &str, origin: &Path, base: &BaseLink) -> Result<Self, BaseLinkError> {
        match Url::parse(link) {
            Ok(url) => {
                Ok(Link::Url(url))
            }
            Err(ParseError::RelativeUrlWithoutBase) => {
                if Path::new(link).is_relative() {
                    let link = if link.starts_with('#') {
                        let file_name = origin.file_name().unwrap().to_string_lossy().to_string().add(link);
                        origin.with_file_name(file_name)
                    } else {
                        origin.with_file_name(link)
                    };
                    Ok(Link::Path(link.to_string_lossy().to_string()))
                } else {
                    base.parse(as_relative(&link).to_string_lossy().to_string().as_str())
                        .map(|base| base.into_link())
                }
            }
            Err(err) => Err(BaseLinkError::from(err)),
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

impl BaseLink {
    fn into_link(self) -> Link {
        self.0
    }
    fn parse(&self, s: &str) -> Result<Self, BaseLinkError> {
        match self.0 {
            Link::Url(ref base) => match base.join(s) {
                Ok(link) => Ok(BaseLink(Link::Url(link))),
                Err(err) => Err(BaseLinkError::ParseError(err)),
            },
            Link::Path(ref base) => Ok(BaseLink(Link::Path(join_absolute(&base, &s).to_string_lossy().to_string()))),
        }
    }
}

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

fn anchor(text: &str) -> String {
    let text = text.nfkc();
    let text = text.map(|c| if c.is_letter() || c.is_number() {
        c
    } else {
        '-'
    });
    let mut was_hyphen = true;
    let text = text.filter(|c| if *c != '-' {
        was_hyphen = false;
        true
    } else if !was_hyphen {
        was_hyphen = true;
        true
    } else {
        was_hyphen = true;
        false
    });
    let mut text: String = text.collect();
    if text.ends_with('-') {
        text.pop();
    }
    text.to_lowercase()
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
                self.linenum += count(&self.buffer.as_bytes()[self.oldoffs..self.parser.get_offset()], b'\n');
                self.oldoffs = self.parser.get_offset();
                return Some((self.linenum, url));
            }
        }
        None
    }
}

pub fn md_file_links<'a>(path: &'a str, links: &mut Vec<(String, usize, String)>) -> io::Result<()> {
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    let parser = MdLinkParser::new(buffer.as_str())
                     .map(|(lineno, url)| (path.to_string(), lineno, url.as_ref().to_string()));

    links.extend(parser);
    Ok(())
}
