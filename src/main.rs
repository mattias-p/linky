extern crate bytecount;
extern crate encoding;
extern crate htmlstream;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate pretty_env_logger;
extern crate pulldown_cmark;
extern crate rayon;
extern crate regex;
extern crate reqwest;
extern crate shell_escape;
#[macro_use]
extern crate structopt;
extern crate url;
extern crate xhtmlchardet;

mod error;
mod linky;

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::io::BufRead;
use std::io;
use std::str::FromStr;

use error::Tag;
use linky::Link;
use linky::Record;
use linky::fetch_link;
use linky::md_file_links;
use linky::parse_link;
use linky::resolve_fragment;
use rayon::prelude::*;
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
    pretty_env_logger::init();
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

    let mut raw_links = vec![];

    if opt.file.is_empty() {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.unwrap();
            raw_links.push(Record::from_str(line.as_str()).unwrap());
        }
    } else {
        for path in &opt.file {
            if let Err(err) = md_file_links(path, &mut raw_links) {
                error!("reading file {}: {}", escape(Cow::Borrowed(path)), err);
            }
        }
    }

    let parsed_links = raw_links.into_iter().filter_map(|record| {
        match parse_link(&record, opt.root.as_str()) {
            Ok((base, fragment)) => Some((record, base, fragment)),
            Err(err) => {
                error!(
                    "{}:{}: {}: {}",
                    record.path, record.linenum, err, record.link
                );
                None
            }
        }
    });

    let mut grouped_links: HashMap<Link, Vec<(Option<String>, Record)>> = HashMap::new();
    for (record, base, fragment) in parsed_links {
        grouped_links
            .entry(base.clone())
            .or_insert_with(|| vec![])
            .push((fragment, record));
    }

    let prefixes: Vec<_> = opt.prefixes.iter().map(AsRef::as_ref).collect();

    let resolved = grouped_links.into_par_iter().flat_map(|(base, links)| {
        let document = client.as_ref().map(|client| fetch_link(client, &base));
        let resolved: Vec<_> = links
            .into_iter()
            .map(|(fragment, record)| match document {
                Some(Ok(ref document)) => {
                    let resolution = resolve_fragment(document, &base, &fragment, &prefixes);
                    (record, Some(resolution))
                }
                Some(Err(ref err)) => (record, Some(Err(err.clone()))),
                None => (record, None),
            })
            .collect();
        resolved
    });

    resolved.for_each(|(record, res)| {
        let tag = res.as_ref()
            .map(|res| res.as_ref().err().map(|err| err.tag()).unwrap_or(Tag::Ok));

        if !tag.as_ref().map_or(false, |tag| silence.contains(&tag)) {
            if let Some(Err(ref err)) = res {
                for line in err.iter() {
                    warn!("{}", line);
                }
            }
            println!(
                "{}:{}: {} {}",
                record.path,
                record.linenum,
                tag.as_ref()
                    .map(|tag| tag as &fmt::Display)
                    .unwrap_or(&"" as &fmt::Display),
                record.link
            );
        }
    })
}
