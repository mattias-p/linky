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

use std::borrow::Cow;
use std::time::Duration;

use reqwest::Client;
use reqwest::RedirectPolicy;
use shell_escape::escape;
use structopt::StructOpt;
use mdlinks::DomainOrPath;
use mdlinks::check_skippable;
use mdlinks::slurp;
use mdlinks::Link;
use mdlinks::LinkIter;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    #[structopt(short = "b", help = "Base domain or path to prefix absolute paths with")]
    base: Option<DomainOrPath>,

    #[structopt(short = "r", help = "Allow redirects")]
    allow_redirects: bool,

    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();

    let mut client = Client::builder();
    client.timeout(Some(Duration::new(5, 0)));
    if !opt.allow_redirects {
        client.redirect(RedirectPolicy::none());
    }
    let client = client.build().unwrap();

    for filename in &opt.file {
        let mut buffer = String::new();
        if let Err(err) = slurp(filename, &mut buffer) {
            eprintln!("{}: error: reading file {}: {}",
                      Opt::clap().get_name(),
                      escape(Cow::from(filename.as_str())),
                      err);
            continue;
        }
        let mut links = LinkIter::new(buffer.as_str());

        while let Some((url, linenum)) = links.next() {
            let link = Link::from(url.as_ref());
            let skippable = check_skippable(&link, Cow::Borrowed(filename), &client, &opt.base);
            if let Err(reason) = skippable {
                println!("{}: {}:{}: {}", reason, filename, linenum, link);
            }
        }
    }
}
