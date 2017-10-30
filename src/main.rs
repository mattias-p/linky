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
use std::io::BufRead;
use std::io;
use std::path::Path;

use linky::BaseLink;
use linky::check_skippable;
use linky::Link;
use linky::md_file_links;
use regex::Regex;
use shell_escape::escape;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    #[structopt(long = "base", short = "b", help = "Join absolute paths to a base URL")]
    base: Option<BaseLink>,

    #[structopt(long = "check", short = "c", help = "Check URLs")]
    check: bool,

    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    let client = if opt.check {
        Some(reqwest::Client::builder()
                 .build()
                 .unwrap())
    } else {
        None
    };

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
        let parsed = if let &Some(ref base) = &opt.base {
            Link::parse_with_base(link.as_str(), &Path::new(&path), base)
        } else {
            Link::parse_with_origin(link.as_str(), &Path::new(&path))
        };
        match parsed {
            Ok(link) => {
                if let &Some(ref client) = &client {
                    if let Err(err) = check_skippable(&link, &client) {
                        println!("{}:{}: {} {}", path, linenum, err, link);
                    };
                } else {
                    println!("{}:{}:  {}", path, linenum, link);
                }
            }
            Err(err) => eprintln!("{}:{}: error: {}: {}", path, linenum, err, link),
        }
    }
}
