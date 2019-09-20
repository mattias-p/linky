mod error;
mod linky;

use std::borrow::Cow;
use std::cmp;
use std::collections::hash_map::Entry;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::io::BufRead;
use std::iter;
use std::iter::FromIterator;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic;
use std::sync::Arc;
use std::sync::Mutex;

use error::Tag;
use linky::fetch_link;
use linky::read_md;
use linky::Client;
use linky::FragResolver;
use linky::Link;
use linky::Record;
use log::debug;
use log::error;
use log::log_enabled;
use log::warn;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
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

    #[structopt(long = "urldecode", short = "u")]
    /// URL-decode local links
    urldecode: bool,

    #[structopt(long = "mute", short = "m")]
    /// Tags to mute
    silence: Vec<Tag>,

    #[structopt(long = "prefix", short = "p")]
    /// Fragment prefixes
    prefixes: Vec<String>,

    #[structopt(long = "root", short = "r", name = "path")]
    /// Join absolute local links to a document root
    root: Option<PathBuf>,

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

fn print_result(
    record: &Record,
    res: &Option<Result<(), Arc<error::Error>>>,
    silence: &HashSet<&Tag>,
) {
    let tag = res
        .as_ref()
        .map(|res| res.as_ref().err().map(|err| err.tag).unwrap_or(Tag::Ok));

    if !tag.as_ref().map_or(false, |tag| silence.contains(&tag)) {
        if let Some(Err(ref err)) = res {
            for line in err.iter() {
                warn!("{}", line);
            }
        }
        println!(
            "{}:{}: {} {}",
            record.doc_path.to_string_lossy(),
            record.doc_line,
            tag.as_ref()
                .map(|tag| tag as &dyn fmt::Display)
                .unwrap_or(&"" as &dyn fmt::Display),
            record.link
        );
    }
}

fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let silence: HashSet<_> = opt.silence.iter().collect();

    let prefixes: Vec<_> = opt.prefixes.iter().map(AsRef::as_ref).collect();
    let resolver = FragResolver::from(&prefixes);
    let client = if opt.check {
        if opt.redirect {
            Some(Client::new_follow())
        } else {
            Some(Client::new_no_follow())
        }
    } else {
        None
    };

    let o = Orderer {
        heap: Mutex::new(BinaryHeap::new()),
        current: atomic::AtomicUsize::new(0),
        f: |(record, res)| {
            print_result(&record, &res, &silence);
        },
    };

    let root: Option<PathBuf> = opt
        .root
        .as_ref()
        .map(|root| fs::canonicalize(root).unwrap());

    if opt.file.is_empty() {
        let stdin = io::stdin();
        let links = stdin
            .lock()
            .lines()
            .map(Result::unwrap)
            .enumerate()
            .map(|(lineno, line)| {
                Record::from_str(&line).map_err(|e| format!("line {}: {}", lineno + 1, e))
            })
            .map(Result::unwrap);
        Box::new(Vec::from_iter(links).into_iter()) as Box<dyn Iterator<Item = _>>
    } else {
        Box::new(opt.file.iter().flat_map(|path| {
            read_md(path)
                .map_err(|err| error!("reading file {}: {}", escape(Cow::Borrowed(path)), err))
                .unwrap_or_else(|_| Box::new(iter::empty()))
        })) as Box<dyn Iterator<Item = _>>
    }
    .filter_map(|record: Record| {
        record
            .to_link(&root)
            .map_err(|err| {
                error!(
                    "{}:{}: {}: {}",
                    record.doc_path.to_string_lossy(),
                    record.doc_line,
                    err,
                    record.link
                )
            })
            .map(|(base, fragment)| Some((record, base, fragment)))
            .unwrap_or(None)
    })
    .enumerate()
    .fold(HashMap::new(), group_fragments)
    .into_par_iter()
    .flat_map(|(base, fragments)| {
        let document = client
            .as_ref()
            .map(|client| fetch_link(client, opt.urldecode, &base));

        // Log all found anchors at the debug level
        if log_enabled!(log::Level::Debug) {
            debug!("In document: {}", &base);
            if let Some(Ok(document)) = &document {
                let mut ids: Vec<_> = document.ids.iter().collect();
                ids.sort_unstable();
                for fragment in ids {
                    debug!("  found anchor: {}", fragment);
                }
            }
        }

        fragments
            .into_iter()
            .map(|(index, fragment, record)| {
                let value = resolver.link(&document, &base, &fragment);
                Item {
                    index,
                    value: (record, value),
                }
            })
            .collect::<Vec<_>>()
    })
    .for_each(|item| o.push(item));
}
