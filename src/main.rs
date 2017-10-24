extern crate bytecount;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate structopt;
extern crate url;
#[macro_use]
extern crate structopt_derive;

use std::borrow::Cow;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use bytecount::count;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
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
    #[structopt(short = "r", help = "Omit relative paths with existing files")]
    omit_existing_file: bool,

    #[structopt(short = "h", help = "Omit http urls (without fragments) that respond with 200-299")]
    omit_existing_basic_http: bool,

    #[structopt(short = "f", help = "Print filename for each link")]
    with_filename: bool,

    #[structopt(short = "n", help = "Print (starting) line number for each link")]
    with_linenum: bool,

    #[structopt(help = "Files to parse")]
    file: Vec<MyPathBuf>,
}

pub struct LinkParser<'a> {
    parser: Parser<'a>,
}

impl<'a> LinkParser<'a> {
    pub fn new(parser: Parser<'a>) -> Self {
        LinkParser {
            parser: parser,
        }
    }

    pub fn get_offset(&self) -> usize {
        self.parser.get_offset()
    }
}

impl<'a> Iterator for LinkParser<'a> {
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

fn main() {
    let opt = Opt::from_args();
    for filename in &opt.file {
        let dir = filename.as_ref().parent().unwrap();
        let filename = filename.as_ref().to_str().unwrap();

        let mut buffer = String::new();
        let mut file = File::open(filename).unwrap();
        file.read_to_string(&mut buffer).unwrap();
        let mut parser = LinkParser::new(Parser::new(buffer.as_str()));

        let mut linenum = 1;
        let mut oldoffs = 0;
        let mut prefix = String::new();
        while let Some(url) = parser.next() {
            let url = Link::from_str(url.as_ref());
            match &url {
                &Link::Path(ref path) => {
                    if opt.omit_existing_file && path.is_relative() {
                        if dir.join(path).exists() {
                            continue;
                        }
                    }
                },
                &Link::Url(ref url) => {
                    if opt.omit_existing_basic_http && (url.scheme() == "http" || url.scheme() == "https") && url.fragment().is_none() {
                        if let Ok(response) = reqwest::get(url.clone()) {
                            if response.status().is_success() {
                                continue;
                            }
                        }
                    }
                },
            }

            prefix.clear();
            if opt.with_filename {
                prefix = format!("{}:", filename);
            }
            if opt.with_linenum {
                linenum += count(&buffer.as_bytes()[oldoffs..parser.get_offset()], b'\n');
                oldoffs = parser.get_offset();
                prefix = format!("{}{}:", prefix, linenum);
            }
            if !prefix.is_empty() {
                prefix = format!("{} ", prefix);
            }
            println!("{}{}", prefix, url);
        }
    }
}
