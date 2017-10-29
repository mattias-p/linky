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

use linky::md_file_links;
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

    for path in &opt.file {
        if let Err(err) = md_file_links(path, &mut links) {
            eprintln!("error: reading file {}: {}",
                      escape(Cow::Borrowed(path)),
                      err);
        }
    }

    for (path, linenum, link) in links {
        println!("{}:{}: {}", path, linenum, link);
    }
}
