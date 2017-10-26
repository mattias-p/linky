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
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
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
enum LinkError {
    Client(reqwest::Error),
    Io(io::Error),
    HttpStatus(StatusCode),
    NoDocument,
    NoAnchor,
    Protocol,
    Absolute,
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
        }
	}

	fn cause(&self) -> Option<&Error> {
		match *self {
            LinkError::Client(ref err) => Some(err),
            LinkError::Io(ref err) => Some(err),
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

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

impl Opt {
    pub fn check_skippable(&self, link: &Link, origin: &Path) -> Result<(), LinkError> {
        match *link {
            Link::Path(ref path) => if PathBuf::from(path).is_relative() {
                if let Some((path, fragment)) = split_fragment(path) {
                    let path = relative_path(path.as_str(), origin).unwrap_or_else(|| PathBuf::from(origin));
                    let mut buffer = String::new();
                    slurp(path.as_path(), &mut buffer)?;
                    if MdAnchorParser::from(buffer.as_str()).any(|anchor| *anchor == fragment) {
                        return Ok(())
                    } else {
                        return Err(LinkError::NoAnchor)
                    }
                } else {
                    if relative_path(path.as_str(), origin).map(|path| path.exists()).unwrap_or(false) {
                        return Ok(())
                    } else {
                        return Err(LinkError::NoDocument)
                    }
                }
            } else {
                Err(LinkError::Absolute)
            },
            Link::Url(ref url) => {
                if url.scheme() == "http" || url.scheme() == "https" {
                    let client = Client::builder()
                        .redirect(RedirectPolicy::none())
                        .timeout(Some(Duration::new(5, 0)))
                        .build();
                    if let Some(fragment) = url.fragment() {
                        let mut response = client.and_then(|client| client.get(url.clone()).send())?;
                        if !response.status().is_success() {
                            Err(LinkError::HttpStatus(response.status()))?;
                        }
                        let mut buffer = String::new();
                        response.read_to_string(&mut buffer)?;
                        for (_, tag) in htmlstream::tag_iter(buffer.as_str()) {
                            for (_, attr) in htmlstream::attr_iter(&tag.attributes) {
                                if attr.value == fragment
                                    && (attr.name == "id"
                                        || (tag.name == "a" && attr.name == "name"))
                                {
                                    return Ok(());
                                }
                            }
                        }
                        return Err(LinkError::NoAnchor)
                    } else {
                        let response = client.and_then(|client| client.head(url.clone()).send())?;
                        if response.status().is_success() {
                            Ok(())
                        } else {
                            Err(LinkError::HttpStatus(response.status()))?
                        }
                    }
                } else {
                    Err(LinkError::Protocol)
                }
            }
        }
    }
}

fn split_fragment(path: &str) -> Option<(String, String)> {
    if let Some(pos) = path.find('#') {
        let mut path = path.to_string();
        let fragment = path.split_off(pos + 1);
        path.pop();
        Some((path, fragment))
    } else {
        None
    }
}

fn relative_path(path: &str, origin: &Path) -> Option<PathBuf> {
    if path.is_empty() {
        None
    } else {
        let base_dir = origin.parent().unwrap();
        Some(base_dir.join(path))
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

fn slurp(filename: &Path, mut buffer: &mut String) -> io::Result<usize> {
    File::open(filename)?.read_to_string(&mut buffer)
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

fn main() {
    let opt = Opt::from_args();
    for filename in &opt.file {
        let mut buffer = String::new();
        if let Err(err) = slurp(Path::new(filename), &mut buffer) {
            eprintln!(
                "{}: error: reading file {}: {}",
                Opt::clap().get_name(),
                escape(Cow::from(filename.as_str())),
                err
            );
            continue;
        }
        let mut parser = MdLinkParser::from(buffer.as_str());

        let mut linenum = 1;
        let mut oldoffs = 0;
        while let Some(url) = parser.next() {
            let link = Link::from(url.as_ref());
            let skippable = opt.check_skippable(&link, Path::new(filename));
            if let Err(reason) = skippable {
                linenum += count(&buffer.as_bytes()[oldoffs..parser.get_offset()], b'\n');
                oldoffs = parser.get_offset();
                println!("{}: {}:{}: {}", reason, filename, linenum, link);
            }
        }
    }
}
