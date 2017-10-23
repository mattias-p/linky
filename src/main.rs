extern crate structopt;
#[macro_use]
extern crate structopt_derive;

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(about = "Extract links from Markdown files.")]
struct Opt {
    /// Needed parameter, the first on the command line.
    #[structopt(help = "Files and/or directory to walk")]
    path: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();
    println!("{:?}", opt);
}
