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

use linky::Link;
use linky::LinkIter;
use linky::slurp;
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
            println!("{}:{}: {}", filename, linenum, link);
        }
    }
}
