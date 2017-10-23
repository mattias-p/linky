extern crate pulldown_cmark;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;

use std::fs::File;
use std::io::Read;

use pulldown_cmark::Event;
use pulldown_cmark::Tag;
use pulldown_cmark::Parser;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    /// Needed parameter, the first on the command line.
    #[structopt(help = "Files to parse")]
    file: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();
    for ref path in &opt.file {
        let mut file = File::open(path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let parser = Parser::new(contents.as_str());
        for event in parser {
            if let Event::Start(Tag::Link(link, _)) = event {
                println!("{}", link);
            }
        }
    }
}
