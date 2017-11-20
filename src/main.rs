extern crate bytecount;
extern crate pretty_env_logger;
extern crate htmlstream;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate pulldown_cmark;
extern crate regex;
extern crate reqwest;
extern crate shell_escape;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate url;

mod errors;
mod linky;

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::io::BufRead;
use std::io;
use std::path::Path;

use linky::BorrowedOrOwned;
use linky::lookup_fragment;
use linky::Link;
use linky::md_file_links;
use linky::Tag;
use linky::Targets;
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

    #[structopt(long = "mute", short = "m", help = "Tags to mute")]
    silence: Vec<Tag>,

    #[structopt(long = "prefix", short = "p", help = "Fragment prefixes")]
    prefixes: Vec<String>,

    #[structopt(long = "root", short = "r", name = "path",
                help = "Join absolute local links to a document root", default_value = "/")]
    root: String,

    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

fn main() {
    pretty_env_logger::init().unwrap();
    let opt = Opt::from_args();
    let silence: HashSet<_> = opt.silence.iter().collect();

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
                error!("reading file {}: {}",
                          escape(Cow::Borrowed(path)),
                          err);
            }
        }
    }

    let links = links.into_iter().filter_map(|(path, linenum, link)| {
        match Link::parse_with_root(link.as_str(), &Path::new(&path), &opt.root) {
            Ok(parsed) => {
                let (base, fragment) = parsed.split_fragment();
                Some((path, linenum, link, base, fragment))
            }
            Err(err) => {
                error!("{}:{}: {}: {}", path, linenum, err, link);
                None
            }
        }
    });

    let mut all_targets = HashMap::new();
    for (path, linenum, raw, base, fragment) in links {
        let status = client.as_ref().map(|client| {
            let targets = all_targets.entry(base.clone())
                                     .or_insert_with(|| client.fetch_targets(&base));
            let prefixes = &opt.prefixes;
            targets.as_ref()
                   .map_err(|err| BorrowedOrOwned::Borrowed(err))
                   .and_then(|ids| lookup_fragment(ids, &fragment, &prefixes).map_err(|err| BorrowedOrOwned::Owned(err)))
                   .err()
        });
        let (tag, err) = match status {
            Some(Some(err)) => (Some(Tag::from_error_kind(err.as_ref().kind())), Some(err)),
            Some(None) => (Some(Tag::ok()), None),
            None => (None, None),
        };
        if !tag.as_ref().map_or(false, |tag| silence.contains(&tag)) {
            if let Some(err) = err {
                warn!("{}", &err.as_ref());
                let mut e = err.as_ref().cause();
                while let Some(err) = e {
                    warn!("  caused by: {}", &err);
                    e = err.cause();
                }
            }
            println!("{}:{}: {} {}", path, linenum, tag.as_ref().map(|tag| tag as &fmt::Display).unwrap_or(&"" as &fmt::Display), raw);
        }
    }
}
