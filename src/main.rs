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
use std::cmp;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::fmt;
use std::io;
use std::io::BufRead;
use std::iter;
use std::iter::FromIterator;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic;

use error::Tag;
use linky::Document;
use linky::Link;
use linky::MdLinkParser;
use linky::Record;
use linky::fetch_link;
use linky::parse_link;
use linky::resolve_fragment;
use linky::slurp;
use rayon::prelude::*;
use shell_escape::escape;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
/// Extract links from Markdown files and check links for brokenness.
struct Opt {
    #[structopt(long = "check", short = "c")]
    /// Check links
    check: bool,

    #[structopt(long = "follow", short = "f")]
    /// Follow HTTP redirects
    redirect: bool,

    #[structopt(long = "mute", short = "m")]
    /// Tags to mute
    silence: Vec<Tag>,

    #[structopt(long = "prefix", short = "p")]
    /// Fragment prefixes
    prefixes: Vec<String>,

    #[structopt(long = "root", short = "r", name = "path", default_value = "/")]
    /// Join absolute local links to a document root
    root: String,

    /// Files to parse
    file: Vec<String>,
}

struct Item<T> {
    index: usize,
    value: T,
}

impl<T> PartialEq for Item<T> {
    fn eq(&self, rhs: &Self) -> bool {
        rhs.index.eq(&self.index)
    }
}

impl<T> Eq for Item<T> {}

impl<T> PartialOrd for Item<T> {
    fn partial_cmp(&self, rhs: &Self) -> Option<cmp::Ordering> {
        rhs.index.partial_cmp(&self.index)
    }
}

impl<T> Ord for Item<T> {
    fn cmp(&self, rhs: &Self) -> cmp::Ordering {
        rhs.index.cmp(&self.index)
    }
}

impl<T> From<(usize, T)> for Item<T> {
    fn from(pair: (usize, T)) -> Self {
        Item {
            index: pair.0,
            value: pair.1,
        }
    }
}

struct Orderer<T, F: Fn(T) -> ()> {
    heap: Mutex<BinaryHeap<Item<T>>>,
    current: atomic::AtomicUsize,
    f: F,
}

impl<T, F: Fn(T) -> ()> Orderer<T, F> {
    fn can_pop(&self, heap: &BinaryHeap<Item<T>>) -> bool {
        let peek_index = heap.peek().map(|item| item.index);
        let current_index = self.current.load(atomic::Ordering::SeqCst);
        Some(current_index) == peek_index
    }
    fn push(&self, item: Item<T>) {
        let mut heap = self.heap.lock().unwrap();
        heap.push(item);
        while self.can_pop(&heap) {
            while self.can_pop(&heap) {
                let value = heap.pop().unwrap().value;
                (self.f)(value);
            }
            self.current.fetch_add(1, atomic::Ordering::SeqCst);
        }
    }
}

fn read_md(path: &str) -> Result<Box<Iterator<Item = Record>>, io::Error> {
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    let parser = MdLinkParser::new(buffer.as_str()).map(|(lineno, url)| Record {
        path: path.to_string(),
        linenum: lineno,
        link: url.as_ref().to_string(),
    });
    Ok(Box::new(parser.collect::<Vec<_>>().into_iter()))
}

fn group_fragments(
    mut acc: HashMap<Link, Vec<(usize, Option<String>, Record)>>,
    link: (usize, (Record, Link, Option<String>)),
) -> HashMap<Link, Vec<(usize, Option<String>, Record)>> {
    let (index, (record, base, fragment)) = link;
    match acc.entry(base.clone()) {
        Entry::Vacant(vacant) => {
            vacant.insert(vec![(index, fragment, record)]);
        }
        Entry::Occupied(mut occupied) => {
            occupied.get_mut().push((index, fragment, record));
        }
    };
    acc
}

fn resolve_link(
    document: &Option<Result<Document, Arc<error::Error>>>,
    prefixes: &[&str],
    base: &Link,
    fragment: &Option<String>,
) -> Option<Result<(), Arc<error::Error>>> {
    match document {
        Some(Ok(ref document)) => {
            let resolution = resolve_fragment(document, base, &fragment, prefixes);
            Some(resolution)
        }
        Some(Err(ref err)) => Some(Err(err.clone())),
        None => None,
    }
}

fn print_result(
    record: &Record,
    res: &Option<Result<(), Arc<error::Error>>>,
    silence: &HashSet<&Tag>,
) {
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
}

fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let silence: HashSet<_> = opt.silence.iter().collect();

    let client = if opt.check {
        let mut builder = reqwest::Client::builder();
        if !opt.redirect {
            builder.redirect(reqwest::RedirectPolicy::none());
        }
        Some(builder.build().unwrap())
    } else {
        None
    };

    let prefixes: Vec<_> = opt.prefixes.iter().map(AsRef::as_ref).collect();

    let o = Orderer {
        heap: Mutex::new(BinaryHeap::new()),
        current: atomic::AtomicUsize::new(0),
        f: |(record, res)| {
            print_result(&record, &res, &silence);
        },
    };

    if opt.file.is_empty() {
        let stdin = io::stdin();
        let links = stdin
            .lock()
            .lines()
            .map(Result::unwrap)
            .map(|line| Record::from_str(&line))
            .map(Result::unwrap);
        Box::new(Vec::from_iter(links).into_iter()) as Box<Iterator<Item = _>>
    } else {
        Box::new(opt.file.iter().flat_map(|path| {
            read_md(path)
                .map_err(|err| error!("reading file {}: {}", escape(Cow::Borrowed(path)), err))
                .unwrap_or_else(|_| Box::new(iter::empty()))
        })) as Box<Iterator<Item = _>>
    }.filter_map(|record| {
        parse_link(&record, opt.root.as_str())
            .map_err(|err| {
                error!(
                    "{}:{}: {}: {}",
                    record.path, record.linenum, err, record.link
                )
            })
            .map(|(base, fragment)| Some((record, base, fragment)))
            .unwrap_or(None)
    })
        .enumerate()
        .fold(HashMap::new(), group_fragments)
        .into_iter()
        .collect::<Vec<_>>()
        .into_par_iter()
        .for_each(|(base, fragments)| {
            let document = client.as_ref().map(|client| fetch_link(client, &base));
            for (index, fragment, record) in fragments {
                let value = resolve_link(&document, &prefixes, &base, &fragment);
                o.push(Item {
                    index,
                    value: (record, value),
                });
            }
        });
}
