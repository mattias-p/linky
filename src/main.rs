extern crate bytecount;
extern crate htmlstream;
extern crate linky;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate shell_escape;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate unicode_categories;
extern crate unicode_normalization;
extern crate url;
extern crate regex;

use std::borrow::Cow;
use std::io;
use std::io::BufRead;

use linky::as_relative;
use linky::BaseLink;
use linky::BaseLinkError;
use linky::Link;
use linky::md_file_links;
use regex::Regex;
use shell_escape::escape;
use structopt::StructOpt;
use url::Url;
use url::ParseError;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    #[structopt(long = "base", help = "Base link to use for relative URLs without base")]
    base: Option<BaseLink>,

    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    let mut links = vec![];

    if opt.file.is_empty() {
        let re = Regex::new(r"^(.*):(\d+): ([^ ]*)$").unwrap();
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.unwrap().as_str().to_string();
            let cap = re.captures(line.as_str()).unwrap();
            let path = cap.get(1).unwrap().as_str();
            let lineno = cap.get(2).unwrap().as_str();
            let link = cap.get(3).unwrap().as_str();

            links.push((path.to_string(), lineno.parse().unwrap(), link.to_string()));
        }
    } else {
        for path in &opt.file {
            if let Err(err) = md_file_links(path, &mut links) {
                eprintln!("error: reading file {}: {}",
                          escape(Cow::Borrowed(path)),
                          err);
            }
        }
    }

    for (path, linenum, link) in links {
        let parsed = match Url::parse(link.as_str()) {
            Err(ParseError::RelativeUrlWithoutBase) => {
                match &opt.base {
                    &Some(ref base) => base.parse(as_relative(&link).to_string_lossy().to_string().as_str()).map(|base| base.into_link()),
                    &None => Link::parse(link.as_str()).map_err(BaseLinkError::from),
                }
            },
            Ok(url) => Ok(Link::Url(url)),
            Err(err) => {
                Err(BaseLinkError::from(err))
            },
        };
        match parsed {
            Ok(link) => println!("{}:{}: {}", path, linenum, link),
            Err(err) => eprintln!("{}:{}: error: {}: {}", path, linenum, err, link),
        }
    }
}
