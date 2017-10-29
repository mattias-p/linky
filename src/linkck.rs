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
extern crate mdlinks;
extern crate regex;

use std::borrow::Cow;
use std::io;
use std::io::BufRead;
use std::time::Duration;

use regex::Regex;
use reqwest::Client;
use reqwest::RedirectPolicy;
use structopt::StructOpt;
use mdlinks::DomainOrPath;
use mdlinks::check_skippable;
use mdlinks::Link;

#[derive(StructOpt, Debug)]
#[structopt(about = "Check link targets.")]
struct Opt {
    #[structopt(short = "b", help = "Base domain or path to prefix absolute paths with")]
    base: Option<DomainOrPath>,

    #[structopt(short = "r", help = "Allow redirects")]
    allow_redirects: bool,
}

fn main() {
    let opt = Opt::from_args();

    let mut client = Client::builder();
    client.timeout(Some(Duration::new(5, 0)));
    if !opt.allow_redirects {
        client.redirect(RedirectPolicy::none());
    }
    let client = client.build().unwrap();

    let pattern = Regex::new(r"^(.*):(\d+): ([^ ]*)$").unwrap();

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let caps = pattern.captures(line.as_str()).unwrap();
        let filename = caps.get(1).unwrap().as_str();
        let linenum = caps.get(2).unwrap().as_str();
        let link = caps.get(3).unwrap().as_str();
        let link = Link::from(link);
        let skippable = check_skippable(&link, Cow::Borrowed(filename), &client, &opt.base);
        if let Err(reason) = skippable {
            println!("{}: {}:{}: {}", reason, filename, linenum, link);
        }
    }
}
