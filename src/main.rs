extern crate bytecount;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate shell_escape;
extern crate structopt;
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
    #[structopt(short = "a", help = "Filter all (implies -l and -h)")]
    filter_all: bool,

    #[structopt(short = "l", help = "Filter existing local links from output")]
    filter_existing_file: bool,

    #[structopt(short = "h", help = "Filter successful HTTP(s) links (without fragments) from output")]
    filter_successful_basic_http: bool,

    #[structopt(short = "f", help = "Include filename and line number for each link")]
    with_filename: bool,

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

#[derive(Debug)]
pub enum Link {
    Url(Url),
    Path(PathBuf),
}

impl Link {
    pub fn from_str(s: &str) -> Self {
        if let Ok(url) = Url::parse(s) {
            Link::Url(url)
        } else {
            Link::Path(PathBuf::from(s))
        }
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Link::Url(ref url) => write!(f, "{}", url),
            &Link::Path(ref path) => write!(f, "{}", path.display()),
        }
    }
}

impl Opt {
    pub fn check_skippable(&self, link: &Link, base_dir: &Path) -> bool {
        match link {
            &Link::Path(ref path) => {
                if self.filter_existing_file && path.is_relative() {
                    if base_dir.join(path).exists() {
                        return true;
                    }
                }
            },
            &Link::Url(ref url) => {
                if self.filter_successful_basic_http && (url.scheme() == "http" || url.scheme() == "https") && url.fragment().is_none() {
                    let response = Client::builder()
                        .redirect(RedirectPolicy::none())
                        .timeout(Some(Duration::new(5, 0)))
                        .build()
                        .and_then(|client|
                            client
                            .head(url.clone())
                            .send()
                        );
                    if let Ok(response) = response {
                        if response.status().is_success() {
                            return true;
                        }
                    }
                }
            },
        }
        false
    }
}

fn slurp(filename: &str, mut buffer: &mut String) -> io::Result<usize> {
    File::open(filename)?.read_to_string(&mut buffer)
}

fn main() {
    let opt = {
        let mut opt = Opt::from_args();
        if opt.filter_all {
            opt.filter_existing_file = true;
            opt.filter_successful_basic_http = true;
        }
        opt
    };
    for filename in &opt.file {
        let dir = filename.as_ref().parent().unwrap();
        let filename = filename.as_ref().to_str().unwrap();

        let mut buffer = String::new();
        if let Err(err) = slurp(filename, &mut buffer) {
            eprintln!("{}: error: reading file {}: {}", Opt::clap().get_name(), escape(Cow::from(filename)), err);
            continue;
        }
        let mut parser = MdLinkParser::from_str(buffer.as_str());

        let mut linenum = 1;
        let mut oldoffs = 0;
        let mut prefix = String::new();
        while let Some(url) = parser.next() {
            let link = Link::from_str(url.as_ref());
            if opt.check_skippable(&link, &dir) {
                continue;
            }

            prefix.clear();
            if opt.with_filename {
                linenum += count(&buffer.as_bytes()[oldoffs..parser.get_offset()], b'\n');
                oldoffs = parser.get_offset();
                prefix = format!("{}:{}: ", filename, linenum);
            }
            println!("{}{}", prefix, link);
        }
    }
}
