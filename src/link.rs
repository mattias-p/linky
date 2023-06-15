use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::result;

use url::Url;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Link {
    Url(Url),
    Path(PathBuf),
}

impl Link {
    pub fn from_url(mut url: Url) -> (Self, Option<String>) {
        let fragment = url.fragment().map(std::string::ToString::to_string);
        url.set_fragment(None);
        (Link::Url(url), fragment)
    }

    pub fn path<P1: AsRef<Path>, P2: AsRef<Path>>(
        link: &str,
        doc_path: &P1,
        base_path: &Option<P2>,
    ) -> result::Result<(Link, Option<String>), url::ParseError> {
        let (path, fragment) = if let Some(pos) = link.find('#') {
            (&link[0..pos], Some(&link[pos + 1..]))
        } else {
            (link, None)
        };
        let path = if Path::new(path).is_absolute() {
            if let Some(base_path) = base_path {
                base_path.as_ref().join(as_relative(&path))
            } else {
                as_relative(&path).into()
            }
        } else if path.is_empty() {
            doc_path.as_ref().into()
        } else {
            doc_path.as_ref().with_file_name(path)
        };
        Ok((
            Link::Path(path),
            fragment.map(std::string::ToString::to_string),
        ))
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Link::Url(ref url) => write!(f, "{url}"),
            Link::Path(ref path) => write!(f, "{}", path.to_string_lossy()),
        }
    }
}

fn as_relative<P: AsRef<Path>>(path: &P) -> &Path {
    let mut components = path.as_ref().components();
    while components.as_path().has_root() {
        components.next();
    }
    components.as_path()
}
