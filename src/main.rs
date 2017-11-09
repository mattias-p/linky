extern crate bytecount;
extern crate htmlstream;
#[macro_use]
extern crate lazy_static;
extern crate pulldown_cmark;
extern crate reqwest;
extern crate shell_escape;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate url;
extern crate regex;

mod linky;

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::BufRead;
use std::io;
use std::path::Path;

use linky::check_skippable_path;
use linky::check_skippable_url;
use linky::GithubId;
use linky::Headers;
use linky::Link;
use linky::LookupTag;
use linky::LookupError;
use linky::md_file_links;
use regex::Regex;
use reqwest::Client;
use reqwest::RedirectPolicy;
use shell_escape::escape;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    #[structopt(long = "check", short = "c", help = "Check links")]
    check: bool,

    #[structopt(long = "follow", short = "f", help = "Follow HTTP redirects")]
    redirect: bool,

    #[structopt(long = "root", short = "r", name = "path", help = "Join absolute local links to a document root", default_value = "/")]
    root: String,

    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    let client = if opt.check {
        let mut builder = Client::builder();
        if !opt.redirect {
            builder.redirect(RedirectPolicy::none());
        }
        Some(builder.build().unwrap())
    } else {
        None
    };

    let mut links = vec![];

    if opt.file.is_empty() {
        let re = Regex::new(r"^(.*):(\d+): [^ ]* ([^ ]*)$").unwrap();
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

    let mut all_headers = HashMap::new();
    for (path, linenum, link) in links {
        match Link::parse_with_root(link.as_str(), &Path::new(&path), &opt.root) {
            Ok(parsed) => {
                let headers = all_headers.entry(path.clone()).or_insert_with(|| Headers::new());
                let status = if let &Some(ref client) = &client {
                    let skippable = match parsed {
                        Link::Path(ref path) => {
                            if Path::new(path).is_relative() {
                                check_skippable_path(path.as_ref(), &GithubId, headers)
                            } else {
                                Err(LookupError::Absolute)
                            }
                        },
                        Link::Url(ref url) => check_skippable_url(url, client),
                    };
                    Some(skippable.err())
                } else {
                    None
                };
                if let Some(tag) = LookupTag(status).display() {
                    println!("{}:{}: {} {}", path, linenum, tag, link);
                }
            }
            Err(err) => eprintln!("{}:{}: error: {}: {}", path, linenum, err, link),
        }
    }
}
