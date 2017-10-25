extern crate bytecount;
extern crate htmlstream;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate shell_escape;
extern crate structopt;
extern crate unicode_categories;
extern crate unicode_normalization;
extern crate url;
#[macro_use]
extern crate structopt_derive;

use std::borrow::Cow;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
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
use shell_escape::escape;
use structopt::StructOpt;
use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;
use url::Url;

pub struct Never(bool);

impl Never {
    pub fn description(&self) -> &str {
        panic!("No instances should exist!");
    }
}

impl fmt::Debug for Never {
    fn fmt(&self, _: &mut std::fmt::Formatter) -> Result<(), fmt::Error> {
        panic!("No instances should exist!");
    }
}

struct MyPathBuf(PathBuf);

impl FromStr for MyPathBuf {
    type Err = Never;
    fn from_str(s: &str) -> Result<MyPathBuf, Never> {
        Ok(MyPathBuf(PathBuf::from(s)))
    }
}

impl AsRef<Path> for MyPathBuf {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl fmt::Debug for MyPathBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), fmt::Error> {
        self.0.fmt(f)
    }
}

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    #[structopt(short = "l", help = "Filter existing local links from output")]
    filter_local: bool,

    #[structopt(short = "r", help = "Filter existing remote links from output")]
    filter_remote: bool,

    #[structopt(short = "F", help = "Omit filename and line number from output")]
    without_filename: bool,

    #[structopt(help = "Files to parse")]
    file: Vec<MyPathBuf>,
}

pub struct MdLinkParser<'a> {
    parser: Parser<'a>,
}

impl<'a> MdLinkParser<'a> {
    pub fn new(parser: Parser<'a>) -> Self {
        MdLinkParser {
            parser: parser,
        }
    }

    pub fn from_str(buffer: &'a str) -> Self {
        MdLinkParser::new(Parser::new(&buffer))
    }

    pub fn get_offset(&self) -> usize {
        self.parser.get_offset()
    }
}

impl<'a> Iterator for MdLinkParser<'a> {
    type Item=Cow<'a, str>;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.parser.next() {
            match event {
            Event::Start(Tag::Link(url, _)) => return Some(url),
            _ => (),
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

    pub fn from_str(buffer: &'a str) -> Self {
        MdAnchorParser::new(Parser::new(&buffer))
    }

    pub fn get_offset(&self) -> usize {
        self.parser.get_offset()
    }
}

impl<'a> Iterator for MdAnchorParser<'a> {
    type Item=String;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.parser.next() {
            match event {
                Event::Start(Tag::Header(_)) => {
                    self.is_header = true;
                }
                Event::Text(text) => {
                    if self.is_header {
                        self.is_header = false;
                        return Some(anchor(text.as_ref()))
                    }
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
    pub fn from_str(s: &str) -> Self {
        if let Ok(url) = Url::parse(s) {
            Link::Url(url)
        } else {
            Link::Path(s.to_string())
        }
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Link::Url(ref url) => write!(f, "{}", url),
            &Link::Path(ref path) => write!(f, "{}", path),
        }
    }
}

impl Opt {
    pub fn check_skippable(&self, link: &Link, filename: &Path) -> bool {
        match link {
            &Link::Path(ref path) => {
                if self.filter_local && PathBuf::from(path).is_relative() {
                    if let Some(pos) = path.find('#') {
                        let mut path = path.clone();
                        let fragment = path.split_off(pos + 1);
                        path.pop();
                        let path = if *path.as_str() == *"" {
                            PathBuf::from(filename)
                        } else {
                            let base_dir = filename.parent().unwrap();
                            base_dir.join(path)
                        };
                        let mut buffer = String::new();
                        if slurp(path.as_path(), &mut buffer).is_err() {
                            return false;
                        }
                        return MdAnchorParser::from_str(buffer.as_str()).any(|anchor|
                            *anchor == fragment
                        )
                    } else if *path != "" {
                        let base_dir = filename.parent().unwrap();
                        return base_dir.join(path).exists();
                    } else {
                        return false;
                    }
                }
            },
            &Link::Url(ref url) => {
                if self.filter_remote && (url.scheme() == "http" || url.scheme() == "https") {
                    let client = Client::builder()
                        .redirect(RedirectPolicy::none())
                        .timeout(Some(Duration::new(5, 0)))
                        .build();
                    if let Some(fragment) = url.fragment() {
                        let response = client.and_then(|client|
                            client
                            .get(url.clone())
                            .send()
                        );
                        if let Ok(mut response) = response {
                            if !response.status().is_success() {
                                return false;
                            }
                            let mut buffer = String::new();
                            if response.read_to_string(&mut buffer).is_err() {
                                return false;
                            }
                            for (_, tag) in htmlstream::tag_iter(buffer.as_str()) {
                                for (_, attr) in htmlstream::attr_iter(&tag.attributes) {
                                    if attr.value == fragment && (attr.name == "id" || (tag.name =="a" && attr.name == "name")) {
                                        return true;
                                    }
                                }
                            }
                            return true;
                        }
                    } else {
                        let response = client.and_then(|client|
                            client
                            .head(url.clone())
                            .send()
                        );
                        if let Ok(response) = response {
                            return response.status().is_success();
                        } else {
                            return false;
                        }
                    }
                }
            },
        }
        false
    }
}

fn slurp(filename: &Path, mut buffer: &mut String) -> io::Result<usize> {
    File::open(filename)?.read_to_string(&mut buffer)
}

pub fn anchor(text: &str) -> String {
    let text = text.nfkc();
    let text = text.map(|c| if c.is_letter() || c.is_number() { c } else { '-' });
    let mut was_hyphen = true;
    let text = text.filter(|c| {
        if *c != '-' {
            was_hyphen = false;
            true
        } else if !was_hyphen {
            was_hyphen = true;
            true
        } else {
            was_hyphen = true;
            false
        }
    });
    let mut text: String = text.collect();
    if text.ends_with("-") {
        text.pop();
    }
    text.to_lowercase()
}

fn main() {
    let opt = Opt::from_args();
    for filename in &opt.file {
        let filename = filename.as_ref().to_str().unwrap();

        let mut buffer = String::new();
        if let Err(err) = slurp(&Path::new(filename), &mut buffer) {
            eprintln!("{}: error: reading file {}: {}", Opt::clap().get_name(), escape(Cow::from(filename)), err);
            continue;
        }
        let mut parser = MdLinkParser::from_str(buffer.as_str());

        let mut linenum = 1;
        let mut oldoffs = 0;
        let mut prefix = String::new();
        while let Some(url) = parser.next() {
            let link = Link::from_str(url.as_ref());
            if opt.check_skippable(&link, &Path::new(filename)) {
                continue;
            }

            prefix.clear();
            if !opt.without_filename {
                linenum += count(&buffer.as_bytes()[oldoffs..parser.get_offset()], b'\n');
                oldoffs = parser.get_offset();
                prefix = format!("{}:{}: ", filename, linenum);
            }
            println!("{}{}", prefix, link);
        }
    }
}
