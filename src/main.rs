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

use linky::Link;
use linky::md_file_links;
use regex::Regex;
use shell_escape::escape;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
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

            links.push((path.to_string(), lineno.parse().unwrap(), Link::from(link)));
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
        println!("{}:{}: {}", path, linenum, link);
    }
}
