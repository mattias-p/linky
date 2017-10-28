extern crate bytecount;
extern crate htmlstream;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate shell_escape;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate unicode_categories;
extern crate unicode_normalization;
extern crate url;

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;


use bytecount::count;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use reqwest::Client;
use reqwest::RedirectPolicy;
use reqwest::StatusCode;
use shell_escape::escape;
use structopt::StructOpt;
use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;
use url::Url;

#[derive(Debug)]
pub enum LinkError {
    Client(reqwest::Error),
    Io(io::Error),
    HttpStatus(StatusCode),
    NoDocument,
    NoAnchor,
    Protocol,
    Absolute,
    Url(url::ParseError),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
            LinkError::Client(_) => write!(f, "CLIENT"),
            LinkError::Io(_) => write!(f, "IO"),
            LinkError::HttpStatus(status) => write!(f, "HTTP_{}", status.as_u16()),
            LinkError::NoDocument => write!(f, "NO_DOCUMENT"),
            LinkError::NoAnchor => write!(f, "NO_ANCHOR"),
            LinkError::Protocol => write!(f, "PROTOCOL"),
            LinkError::Absolute => write!(f, "ABSOLUTE"),
            LinkError::Url(_) => write!(f, "URL"),
        }
    }
}

impl Error for LinkError {

	fn description(&self) -> &str {
		match *self {
            LinkError::Client(ref err) => err.description(),
            LinkError::Io(ref err) => err.description(),
            LinkError::HttpStatus(_) => "unexpected http status",
            LinkError::NoDocument => "document not found",
            LinkError::NoAnchor => "anchor not found",
            LinkError::Protocol => "unrecognized protocol",
            LinkError::Absolute => "unhandled absolute path",
            LinkError::Url(_) => "invalid url",
        }
	}

	fn cause(&self) -> Option<&Error> {
		match *self {
            LinkError::Client(ref err) => Some(err),
            LinkError::Io(ref err) => Some(err),
            LinkError::Url(ref err) => Some(err),
            _ => None,
        }
	}
}

impl From<io::Error> for LinkError {
    fn from(err: io::Error) -> Self {
        LinkError::Io(err)
    }
}

impl From<reqwest::Error> for LinkError {
    fn from(err: reqwest::Error) -> Self {
        LinkError::Client(err)
    }
}

impl From<url::ParseError> for LinkError {
    fn from(err: url::ParseError) -> Self {
        LinkError::Url(err)
    }
}

#[derive(Debug)]
pub struct DomainOrPathError;

impl fmt::Display for DomainOrPathError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Contains path, query and/or fragment")
    }
}

impl Error for DomainOrPathError {
	fn description(&self) -> &str {
        "bad parts"
    }
	fn cause(&self) -> Option<&Error> {
        None
    }
}

#[derive(Debug)]
pub struct DomainOrPath(Link);

impl FromStr for DomainOrPath {
    type Err = DomainOrPathError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let link = Link::from(s);
        match link {
            Link::Url(ref url) if url.path_segments().is_some() && url.query().is_some() && url.fragment().is_some() => Err(DomainOrPathError),
            _ => Ok(DomainOrPath(link)),
        }
    }
}

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {

    #[structopt(short = "b", help = "Base domain or path to prefix absolute paths with")]
    base: Option<DomainOrPath>,

    #[structopt(short = "r", help = "Allow redirects")]
    allow_redirects: bool,

    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

pub fn check_skippable<'a>(link: &Link, origin: Cow<'a, str>, client: &Client, base: &Option<DomainOrPath>) -> Result<(), LinkError> {
    match *link {
        Link::Path(ref path) => {
            if PathBuf::from(path).is_relative() {
                let path = relative_path(path, origin);
                check_skippable_path(path.as_ref())
            } else if let Some(DomainOrPath(Link::Path(ref base_path))) = *base {
                let path = join_absolute(base_path, path);
                check_skippable_path(path.to_string_lossy().as_ref())
            } else if let Some(DomainOrPath(Link::Url(ref base_domain))) = *base {
                check_skippable_url(&base_domain.join(path)?, client)
            } else {
                Err(LinkError::Absolute)
            }
        },
        Link::Url(ref url) => check_skippable_url(url, client),
    }
}

fn check_skippable_path(path: &str) -> Result<(), LinkError> {
    if let Some((path, fragment)) = split_fragment(path) {
        let mut buffer = String::new();
        slurp(&path, &mut buffer)?;
        if MdAnchorParser::from(buffer.as_str()).any(|anchor| *anchor == *fragment) {
            Ok(())
        } else {
            Err(LinkError::NoAnchor)
        }
    } else {
        if Path::new(path).exists() {
            Ok(())
        } else {
            Err(LinkError::NoDocument)
        }
    }
}

fn check_skippable_url(url: &Url, client: &Client) -> Result<(), LinkError> {
    if url.scheme() == "http" || url.scheme() == "https" {
        if let Some(fragment) = url.fragment() {
            let mut response = client.get(url.clone()).send()?;
            if !response.status().is_success() {
                Err(LinkError::HttpStatus(response.status()))?;
            }
            let mut buffer = String::new();
            response.read_to_string(&mut buffer)?;
            if has_html_anchor(&buffer, fragment) {
                Ok(())
            } else {
                Err(LinkError::NoAnchor)
            }
        } else {
            let response = client.head(url.clone()).send()?;
            if response.status().is_success() {
                Ok(())
            } else {
                Err(LinkError::HttpStatus(response.status()))
            }
        }
    } else {
        Err(LinkError::Protocol)
    }
}

fn join_absolute<P1: AsRef<Path>, P2: AsRef<Path>>(base_path: &P1, path: &P2) -> PathBuf {
    let mut components = path.as_ref().components();
    while components.as_path().has_root() {
        components.next();
    }
    base_path.as_ref().join(components.as_path())
}

fn has_html_anchor(buffer: &str, anchor: &str) -> bool {
    for (_, tag) in htmlstream::tag_iter(buffer) {
        for (_, attr) in htmlstream::attr_iter(&tag.attributes) {
            if attr.value == anchor
                && (attr.name == "id"
                    || (tag.name == "a" && attr.name == "name"))
            {
                return true;
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

fn relative_path<'a>(path: &'a str, origin: Cow<'a, str>) -> Cow<'a, str> {
    if path.is_empty() {
        origin
    } else {
        let base_dir = Path::new(origin.as_ref()).parent().unwrap();
        let path = base_dir.join(path).to_string_lossy().into_owned();
        Cow::Owned(path)
    }
}

pub struct MdLinkParser<'a> {
    parser: Parser<'a>,
}

impl<'a> MdLinkParser<'a> {
    pub fn new(parser: Parser<'a>) -> Self {
        MdLinkParser { parser: parser }
    }

    pub fn get_offset(&self) -> usize {
        self.parser.get_offset()
    }
}

impl<'a> From<&'a str> for MdLinkParser<'a> {
    fn from(buffer: &'a str) -> Self {
        MdLinkParser::new(Parser::new(buffer))
    }
}

impl<'a> Iterator for MdLinkParser<'a> {
    type Item = Cow<'a, str>;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.parser.next() {
            if let Event::Start(Tag::Link(url, _)) = event {
                return Some(url);
            }
        }
        None
    }
}

pub struct MdAnchorParser<'a> {
    parser: Parser<'a>,
    is_header: bool,
}

impl<'a> MdAnchorParser<'a> {
    pub fn new(parser: Parser<'a>) -> Self {
        MdAnchorParser {
            parser: parser,
            is_header: false,
        }
    }

    pub fn get_offset(&self) -> usize {
        self.parser.get_offset()
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

impl<'a> From<&'a str> for Link {
    fn from(s: &str) -> Self {
        if let Ok(url) = Url::parse(s) {
            Link::Url(url)
        } else {
            Link::Path(s.to_string())
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

fn slurp<P: AsRef<Path>>(filename: &P, mut buffer: &mut String) -> io::Result<usize> {
    File::open(filename.as_ref())?.read_to_string(&mut buffer)
}

pub fn anchor(text: &str) -> String {
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

struct LinkIter<'a> {
    buffer: &'a str,
    parser: MdLinkParser<'a>,
    linenum: usize,
    oldoffs: usize,
}

impl<'a> LinkIter<'a> {
    fn new(buffer: &'a str) -> Self {
        LinkIter {
            parser: MdLinkParser::from(buffer),
            buffer: buffer,
            linenum: 1,
            oldoffs: 0,
        }
    }
}

impl<'a> Iterator for LinkIter<'a> {
    type Item = (Cow<'a, str>, usize);
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(url) = self.parser.next() {
            self.linenum += count(&self.buffer.as_bytes()[self.oldoffs..self.parser.get_offset()], b'\n');
            self.oldoffs = self.parser.get_offset();
            return Some((url, self.linenum));
        }
        None
    }
}

fn main() {
    let opt = Opt::from_args();

    let mut client = Client::builder();
    client.timeout(Some(Duration::new(5, 0)));
    if !opt.allow_redirects {
        client.redirect(RedirectPolicy::none());
    }
    let client = client.build().unwrap();

    for filename in &opt.file {
        let mut buffer = String::new();
        if let Err(err) = slurp(filename, &mut buffer) {
            eprintln!(
                "{}: error: reading file {}: {}",
                Opt::clap().get_name(),
                escape(Cow::from(filename.as_str())),
                err
            );
            continue;
        }
        let mut links = LinkIter::new(buffer.as_str());

        while let Some((url, linenum)) = links.next() {
            let link = Link::from(url.as_ref());
            let skippable = check_skippable(&link, Cow::Borrowed(filename), &client, &opt.base);
            if let Err(reason) = skippable {
                println!("{}: {}:{}: {}", reason, filename, linenum, link);
            }
        }
    }
}
