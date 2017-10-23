extern crate pulldown_cmark;
extern crate structopt;
extern crate url;
#[macro_use]
extern crate structopt_derive;

use std::fs::File;
use std::io::Read;

use pulldown_cmark::Event;
use pulldown_cmark::Tag;
use pulldown_cmark::Parser;
use structopt::StructOpt;
use url::Url;
use url::ParseError::RelativeUrlWithoutBase;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    #[structopt(short = "b", help = "Base url")]
    base: Option<Url>,

    #[structopt(help = "Files to parse")]
    file: Vec<String>,
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

fn main() {
    let opt = Opt::from_args();
    for ref path in &opt.file {
        let mut file = File::open(path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let parser = Parser::new(contents.as_str());
        for event in parser {
            if let Event::Start(Tag::Link(url, _)) = event {
                let url = parse_url(&url, &opt.base).unwrap();
                println!("{}", url);
            }
        }
    }
}
