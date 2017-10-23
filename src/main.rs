extern crate pulldown_cmark;
extern crate structopt;
extern crate url;
#[macro_use]
extern crate structopt_derive;

use std::env::current_dir;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag;
use structopt::StructOpt;
use url::ParseError::RelativeUrlWithoutBase;
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
    #[structopt(short = "u", help = "Base URL")]
    base_url: Option<Url>,

    #[structopt(short = "d", help = "Base directory")]
    base_dir: Option<MyPathBuf>,

    #[structopt(help = "Files to parse")]
    file: Vec<MyPathBuf>,
}

fn parse_url(url: &str, base: &Option<Url>) -> Result<Url, url::ParseError> {
    match Url::parse(&url) {
        err @ Err(RelativeUrlWithoutBase) => {
            if let &Some(ref base) = base {
                base.join(&url)
            } else {
                err
            }
        },
        other @ _ => other,
    }
}

fn abs_path(path: &Path) -> PathBuf {
    if path.is_relative() {
        let base_dir = current_dir().unwrap();
        base_dir.join(path)
    } else {
        path.to_path_buf()
    }
}

fn aug_base_url(base_url: Url, base_dir: &Path, path: &Path) -> Result<Url, url::ParseError> {
    if let Ok(suffix) = path.strip_prefix(base_dir) {
        let suffix = suffix.to_str().unwrap();
        base_url.join(suffix)
    } else {
        Ok(base_url)
    }
}

fn main() {
    let opt = Opt::from_args();
    let base_dir: Option<PathBuf> = opt.base_dir.map(|ref base_dir| abs_path(base_dir.as_ref()));
    for path in &opt.file {
        let path = abs_path(path.as_ref());
        let base_url = if let &Some(ref base_url) = &opt.base_url {
            if let &Some(ref base_dir) = &base_dir {
                Some(aug_base_url(base_url.clone(), &base_dir, &path).unwrap())
            } else {
                Some(base_url.clone())
            }
        } else {
            None
        };
        let mut file = File::open(path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let parser = Parser::new(contents.as_str());

        for event in parser {
            if let Event::Start(Tag::Link(url, _)) = event {
                let url = parse_url(&url, &base_url).unwrap();
                println!("{}", url);
            }
        }
    }
}
