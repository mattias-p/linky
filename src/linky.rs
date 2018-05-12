use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::ops::Add;
use std::path::Path;
use std::result;
use std::str::FromStr;
use std::sync::Arc;

use bytecount::count;
use encoding::DecoderTrap;
use encoding::label::encoding_from_whatwg_label;
use error::Error;
use error::Result;
use error::Tag;
use htmlstream;
use pulldown_cmark;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use regex::Regex;
use reqwest::Client;
use reqwest::header::ContentType;
use reqwest::mime;
use url;
use url::Url;
use xhtmlchardet;

lazy_static! {
    static ref MARKDOWN_CONTENT_TYPE: ContentType =
        ContentType("text/markdown; charset=UTF-8".parse().unwrap());
}

enum Format {
    Html,
    Markdown,
}

pub struct Document<'a> {
    pub ids: HashSet<Cow<'a, str>>,
}

impl<'a> Document<'a> {
    pub fn empty() -> Self {
        Document {
            ids: HashSet::new(),
        }
    }

    #[cfg(test)]
    pub fn new() -> Self {
        Self::from(&[])
    }

    #[cfg(test)]
    pub fn from(ids: &'a [&'a str]) -> Self {
        Document {
            ids: [""].iter().chain(ids).cloned().map(Cow::from).collect(),
        }
    }

    fn parse<R: Read>(mut reader: R, content_type: &ContentType) -> Result<Document<'a>> {
        let format = match (content_type.type_(), content_type.subtype().as_str()) {
            (mime::TEXT, "html") => Format::Html,
            (mime::TEXT, "markdown") => Format::Markdown,
            _ => {
                return Ok(Document::empty());
            }
        };

        let charset_hint = content_type
            .get_param(mime::CHARSET)
            .map(|v| v.as_ref().to_string());
        debug!("http charset hint: {:?}", &charset_hint);

        let chars = read_chars(&mut reader, charset_hint)?;

        let ids = match format {
            Format::Markdown => {
                let mut headers = Headers::new();
                MdAnchorParser::from_buffer(&chars, &GithubId, &mut headers)
                    .map(Cow::from)
                    .collect()
            }
            Format::Html => {
                let mut result = HashSet::new();
                for (_, tag) in htmlstream::tag_iter(&chars) {
                    for (_, attr) in htmlstream::attr_iter(&tag.attributes) {
                        if attr.name == "id" || (tag.name == "a" && attr.name == "name") {
                            result.insert(Cow::from(attr.value));
                        }
                    }
                }
                result
            }
        };

        Ok(Document { ids })
    }
}

pub struct FragResolver<'a> {
    prefixes: HashSet<Cow<'a, str>>,
}

impl<'a> FragResolver<'a> {
    #[cfg(test)]
    pub fn new() -> Self {
        FragResolver {
            prefixes: HashSet::new(),
        }
    }

    pub fn from(prefixes: &'a [&'a str]) -> Self {
        FragResolver {
            prefixes: prefixes.iter().cloned().map(Cow::from).collect(),
        }
    }

    fn find_prefix<'d>(&self, fragment: &str, document: &Document<'d>) -> Option<&str> {
        if document.ids.contains(&Cow::from(fragment)) {
            return Some("");
        }
        for prefix in &self.prefixes {
            if document
                .ids
                .contains(format!("{}{}", prefix, fragment).as_str())
            {
                return Some(prefix);
            }
        }
        None
    }

    pub fn fragment(&self, document: &Document, fragment: &str) -> Result<()> {
        self.find_prefix(fragment, document)
            .ok_or_else(|| {
                Error::root(Tag::NoFragment).context(Cow::from(format!("fragment = #{}", fragment)))
            })
            .and_then(|prefix| {
                if prefix == "" {
                    Ok(())
                } else {
                    Err(Error::root(Tag::Prefixed)
                        .context(Cow::from(format!("prefix = {}", prefix)))
                        .context(Cow::from(format!("fragment = #{}", &fragment))))
                }
            })
    }

    pub fn link(
        &self,
        document: &Option<result::Result<Document, Arc<Error>>>,
        base: &Link,
        fragment: &Option<String>,
    ) -> Option<result::Result<(), Arc<Error>>> {
        document.as_ref().map(|document| {
            document
                .as_ref()
                .map_err(|err| err.clone())
                .and_then(|document| {
                    if let Some(ref fragment) = *fragment {
                        self.fragment(document, fragment).map_err(|err| {
                            Arc::new(err.context(Cow::from(format!("link = {}", base))))
                        })
                    } else {
                        Ok(())
                    }
                })
        })
    }
}

trait LocalResolver {
    fn local(&self, path: &Path) -> Result<Document>;
}

trait RemoteResolver {
    fn remote<'b>(&self, url: &Url) -> Result<Document<'b>>;
}

struct FilesystemLocalResolver;

impl LocalResolver for FilesystemLocalResolver {
    fn local(&self, path: &Path) -> Result<Document> {
        if path.is_absolute() {
            Err(Tag::Absolute.into())
        } else if path.is_dir() {
            Err(Tag::Directory.into())
        } else {
            let reader = File::open(&path)?;
            Document::parse(reader, &MARKDOWN_CONTENT_TYPE)
        }
    }
}

struct NetworkRemoteResolver<'a>(&'a Client);

impl<'a> RemoteResolver for NetworkRemoteResolver<'a> {
    fn remote<'b>(&self, url: &Url) -> Result<Document<'b>> {
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(Tag::Protocol.into());
        }

        let response = self.0.get(url.clone()).send()?;

        if !response.status().is_success() {
            return Err(Tag::HttpStatus(response.status()).into());
        }
        let content_type: Result<ContentType> = response
            .headers()
            .get::<ContentType>()
            .cloned()
            .ok_or_else(|| Tag::NoMime.into());
        let content_type = content_type?;

        Document::parse(response, &content_type)
    }
}

fn as_relative<P: AsRef<Path>>(path: &P) -> &Path {
    let mut components = path.as_ref().components();
    while components.as_path().has_root() {
        components.next();
    }
    components.as_path()
}

fn split_fragment(path: &str) -> Option<(&str, &str)> {
    if let Some(pos) = path.find('#') {
        Some((&path[0..pos], &path[pos + 1..]))
    } else {
        None
    }
}

fn split_path_fragment(path: &str) -> (&str, Option<&str>) {
    if let Some((path, fragment)) = split_fragment(path) {
        (path, Some(fragment))
    } else {
        (path, None)
    }
}

fn split_url_fragment(url: &Url) -> (&Url, Option<&str>) {
    (url, url.fragment())
}

struct MdAnchorParser<'a> {
    parser: Parser<'a>,
    is_header: bool,
    headers: &'a mut Headers,
    id_transform: &'a ToId,
}

impl<'a> MdAnchorParser<'a> {
    fn new(parser: Parser<'a>, id_transform: &'a ToId, headers: &'a mut Headers) -> Self {
        MdAnchorParser {
            parser,
            is_header: false,
            headers,
            id_transform,
        }
    }

    fn from_buffer(buffer: &'a str, id_transform: &'a ToId, headers: &'a mut Headers) -> Self {
        MdAnchorParser::new(Parser::new(buffer), id_transform, headers)
    }
}

impl<'a> Iterator for MdAnchorParser<'a> {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.parser.next() {
            match event {
                Event::Start(pulldown_cmark::Tag::Header(_)) => {
                    self.is_header = true;
                }
                Event::Text(text) => {
                    if self.is_header {
                        self.is_header = false;
                        let count = self.headers.register(text.to_string());
                        return Some(self.id_transform.to_id(text.as_ref(), count));
                    }
                }
                _ => (),
            }
        }
        None
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Link {
    Url(Url),
    Path(String),
}

unsafe impl Sync for Link {}

impl Link {
    pub fn split_fragment(&self) -> (Link, Option<String>) {
        match *self {
            Link::Path(ref path) => {
                let (path, fragment) = split_path_fragment(path);
                (
                    Link::Path(path.to_string()),
                    fragment.map(|f| f.to_string()),
                )
            }
            Link::Url(ref url) => {
                let (url, fragment) = split_url_fragment(url);
                let mut url = url.clone();
                url.set_fragment(None);
                (Link::Url(url), fragment.map(|f| f.to_string()))
            }
        }
    }

    pub fn parse_with_root<P1: AsRef<Path>, P2: AsRef<Path>>(
        link: &str,
        origin: &P1,
        root: &P2,
    ) -> result::Result<Self, url::ParseError> {
        match Url::parse(link) {
            Ok(url) => Ok(Link::Url(url)),
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                if Path::new(link).is_relative() {
                    let link = if link.starts_with('#') {
                        let file_name = origin
                            .as_ref()
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .to_string()
                            .add(link);
                        origin.as_ref().with_file_name(file_name)
                    } else {
                        origin.as_ref().with_file_name(link)
                    };
                    Ok(Link::Path(link.to_string_lossy().to_string()))
                } else {
                    Ok(Link::Path(
                        root.as_ref()
                            .join(as_relative(&link))
                            .to_string_lossy()
                            .to_string(),
                    ))
                }
            }
            Err(err) => Err(err),
        }
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Link::Url(ref url) => write!(f, "{}", url),
            Link::Path(ref path) => write!(f, "{}", path),
        }
    }
}

fn read_chars(reader: &mut Read, charset_hint: Option<String>) -> Result<String> {
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    let mut cursor = Cursor::new(buffer);

    let charsets = xhtmlchardet::detect(&mut cursor, charset_hint)?;

    debug!("detected charsets: {:?}", &charsets);

    let charset = charsets
        .iter()
        .flat_map(|v| encoding_from_whatwg_label(v.as_str()))
        .next()
        .ok_or_else(|| Error::decoding_error(Cow::from("Failed to detect character encoding")))?;

    charset
        .decode(cursor.into_inner().as_ref(), DecoderTrap::Strict)
        .map_err(|err| {
            Error::decoding_error(err).context(Cow::from(format!("encoding = {}", &charset.name())))
        })
}

pub fn slurp<P: AsRef<Path>>(filename: &P, mut buffer: &mut String) -> io::Result<usize> {
    File::open(filename.as_ref())?.read_to_string(&mut buffer)
}

lazy_static! {
    static ref GITHUB_PUNCTUATION: Regex = Regex::new(r"[^\w -]").unwrap();
}

trait ToId {
    fn to_id(&self, text: &str, repetition: usize) -> String;
}

struct GithubId;

impl ToId for GithubId {
    fn to_id(&self, text: &str, repetition: usize) -> String {
        let text = GITHUB_PUNCTUATION.replace_all(text, "");
        let text = text.to_ascii_lowercase();
        let text = text.replace('-', "-");
        if repetition == 0 {
            text
        } else {
            format!("{}-{}", text, repetition)
        }
    }
}

struct Headers(HashMap<String, usize>);

impl Headers {
    fn new() -> Self {
        Headers(HashMap::new())
    }

    fn register(&mut self, text: String) -> usize {
        match self.0.entry(text) {
            Entry::Occupied(ref mut occupied) => {
                let count = *occupied.get();
                *occupied.get_mut() = count + 1;
                count
            }
            Entry::Vacant(vacant) => {
                vacant.insert(1);
                0
            }
        }
    }
}

pub struct MdLinkParser<'a> {
    buffer: &'a str,
    parser: Parser<'a>,
    linenum: usize,
    oldoffs: usize,
}

impl<'a> MdLinkParser<'a> {
    pub fn new(buffer: &'a str) -> Self {
        MdLinkParser {
            parser: Parser::new(buffer),
            buffer,
            linenum: 1,
            oldoffs: 0,
        }
    }
}

impl<'a> Iterator for MdLinkParser<'a> {
    type Item = (usize, Cow<'a, str>);
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(event) = self.parser.next() {
            if let Event::Start(pulldown_cmark::Tag::Link(url, _)) = event {
                self.linenum += count(
                    &self.buffer.as_bytes()[self.oldoffs..self.parser.get_offset()],
                    b'\n',
                );
                self.oldoffs = self.parser.get_offset();
                return Some((self.linenum, url));
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct Record {
    pub path: String,
    pub linenum: usize,
    pub link: String,
}

unsafe impl Sync for Record {}

lazy_static! {
    static ref RECORD_REGEX: Regex = Regex::new(r"^(.*):(\d+): [^ ]* ([^ ]*)$").unwrap();
}

impl FromStr for Record {
    type Err = ();
    fn from_str(line: &str) -> result::Result<Self, Self::Err> {
        let cap = RECORD_REGEX.captures(line).ok_or(())?;
        Ok(Record {
            path: cap.get(1).unwrap().as_str().to_string(),
            linenum: cap.get(2).unwrap().as_str().parse().unwrap(),
            link: cap.get(3).unwrap().as_str().to_string(),
        })
    }
}

pub fn parse_link(
    record: &Record,
    root: &str,
) -> result::Result<(Link, Option<String>), url::ParseError> {
    Link::parse_with_root(record.link.as_str(), &Path::new(&record.path), &root)
        .map(|parsed| parsed.split_fragment())
}

pub fn fetch_link<'a>(client: &Client, link: &Link) -> result::Result<Document<'a>, Arc<Error>> {
    match *link {
        Link::Path(ref path) => FilesystemLocalResolver.local(path.as_ref()),
        Link::Url(ref url) => NetworkRemoteResolver(client).remote(url),
    }.map_err(|err| Arc::new(err.context(Cow::from(format!("link = {}", link)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_links() {
        let buffer = include_str!("../example_site/path/to/example.md");
        let mut parser = MdLinkParser::new(buffer);
        assert_eq!(
            parser.next(),
            Some((
                2,
                Cow::Owned(
                    "https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md"
                        .to_string()
                )
            ))
        );
        assert_eq!(parser.next(), Some((3, Cow::Owned("https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing".to_string()))));
        assert_eq!(parser.next(), Some((4, Cow::Owned("other.md".to_string()))));
        assert_eq!(
            parser.next(),
            Some((5, Cow::Owned("non-existing.md".to_string())))
        );
        assert_eq!(
            parser.next(),
            Some((6, Cow::Owned("other.md#existing".to_string())))
        );
        assert_eq!(
            parser.next(),
            Some((7, Cow::Owned("other.md#non-existing".to_string())))
        );
        assert_eq!(parser.next(), Some((8, Cow::Owned("#heading".to_string()))));
        assert_eq!(
            parser.next(),
            Some((9, Cow::Owned("#non-existing".to_string())))
        );
        assert_eq!(parser.next(), None);
    }

    #[test]
    fn check_fragment() {
        assert_eq!(
            lookup_fragment(&Document::from(&["abc"]), "abc", &FragResolver::new())
                .map_err(|e| e.tag()),
            Ok(())
        );
        assert_eq!(
            lookup_fragment(&Document::new(), "", &FragResolver::new()).map_err(|e| e.tag()),
            Ok(())
        );
        assert_eq!(
            lookup_fragment(&Document::empty(), "", &FragResolver::new()).map_err(|e| e.tag()),
            Err(Tag::NoFragment)
        );
        assert_eq!(
            lookup_fragment(
                &Document::from(&["prefix"]),
                "",
                &FragResolver::from(&["prefix"])
            ).map_err(|e| e.tag()),
            Ok(())
        );
        assert_eq!(
            lookup_fragment(&Document::new(), "abc", &FragResolver::new()).map_err(|e| e.tag()),
            Err(Tag::NoFragment)
        );
        assert_eq!(
            lookup_fragment(
                &Document::from(&["abc-123"]),
                "123",
                &FragResolver::from(&["abc-"])
            ).map_err(|e| e.tag()),
            Err(Tag::Prefixed)
        );
    }

    #[test]
    fn find_fragments() {
        assert_eq!(FragResolver::new().fragment("123", &Document::new()), None);

        assert_eq!(
            FragResolver::new().fragment("123", &Document::from(&["abc-123"])),
            None
        );

        assert_eq!(
            FragResolver::from(&["def-"]).fragment("123", &Document::from(&["abc-123"])),
            None
        );

        assert_eq!(
            FragResolver::from(&["abc-"]).fragment("123", &Document::from(&["abc-123"])),
            Some("abc-")
        );

        assert_eq!(
            FragResolver::from(&["abc-", "def-"]).fragment("123", &Document::from(&["def-123"])),
            Some("def-")
        );
    }

    #[test]
    fn decoding() {
        let latin1 = b"\xC4ntligen stod pr\xE4sten i predikstolen.".to_vec();
        assert_eq!(
            read_chars(&mut latin1.as_slice(), Some("ISO-8859-1".to_string())).ok(),
            Some("Äntligen stod prästen i predikstolen.".to_string())
        );
    }
}
