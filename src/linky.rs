use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::fs::File;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::result;
use std::str::FromStr;
use std::sync;

use bytecount::count;
use encoding::label::encoding_from_whatwg_label;
use encoding::DecoderTrap;
use lazy_static::lazy_static;
use log::debug;
use pulldown_cmark::CowStr;
use pulldown_cmark::Event;
use pulldown_cmark::OffsetIter;
use pulldown_cmark::Parser;
use regex::Regex;
use reqwest::header::HeaderValue;
use reqwest::header::CONTENT_TYPE;
use url::Url;

use crate::error::Error;
use crate::error::Result;
use crate::error::Tag;

lazy_static! {
    static ref MARKDOWN_CONTENT_TYPE: mime::Mime = "text/markdown; charset=UTF-8".parse().unwrap();
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

    fn parse<R: Read>(mut reader: R, content_type: &mime::Mime) -> Result<Document<'a>> {
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

    fn find_prefix(&self, fragment: &str, document: &Document<'_>) -> Option<&str> {
        if document.ids.contains(&Cow::from(fragment)) {
            return Some("");
        }
        self.prefixes
            .iter()
            .find(|&prefix| {
                document
                    .ids
                    .contains(format!("{prefix}{fragment}").as_str())
            })
            .map(AsRef::as_ref)
    }

    pub fn fragment(&self, document: &Document, fragment: &str) -> Result<()> {
        match self.find_prefix(fragment, document) {
            Some(prefix) => Ok(prefix),
            None => {
                let fragment_lc = fragment.to_lowercase();
                let temp = if fragment_lc != fragment {
                    self.find_prefix(&fragment_lc, document).map(|_| {
                        Err(Tag::CaseInsensitiveFragment
                            .as_error()
                            .context(Cow::from(format!("anchor = #{fragment_lc}")))
                            .context(Cow::from(format!("fragment = #{fragment}"))))
                    })
                } else {
                    None
                };
                match temp {
                    Some(res) => res,
                    None => Err(Tag::NoFragment
                        .as_error()
                        .context(Cow::from(format!("fragment = #{fragment}")))),
                }
            }
        }
        .and_then(|prefix| {
            if prefix.is_empty() {
                Ok(())
            } else {
                Err(Tag::Prefixed
                    .as_error()
                    .context(Cow::from(format!("prefix = {prefix}")))
                    .context(Cow::from(format!("fragment = #{fragment}"))))
            }
        })
    }

    pub fn link(
        &self,
        document: &Option<result::Result<Document, sync::Arc<Error>>>,
        base: &Link,
        fragment: &Option<String>,
    ) -> Option<result::Result<(), sync::Arc<Error>>> {
        document.as_ref().map(|document| {
            document
                .as_ref()
                .map_err(std::clone::Clone::clone)
                .and_then(|document| {
                    if let Some(ref fragment) = *fragment {
                        self.fragment(document, fragment).map_err(|err| {
                            sync::Arc::new(err.context(Cow::from(format!("link = {base}"))))
                        })
                    } else {
                        Ok(())
                    }
                })
        })
    }
}

pub struct Client {
    inner: reqwest::blocking::Client,
    redirects: sync::Arc<sync::Mutex<Vec<(reqwest::StatusCode, reqwest::Url)>>>,
}

impl Client {
    pub fn new_no_follow() -> Self {
        let redirects = sync::Arc::new(sync::Mutex::new(vec![]));
        let redirects_clone = redirects.clone();
        let inner = reqwest::blocking::Client::builder()
            .user_agent("linky")
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                let mut redirects_guard = redirects_clone.lock().unwrap();
                redirects_guard.push((attempt.status(), attempt.url().clone()));
                reqwest::redirect::Policy::default().redirect(attempt)
            }))
            .build()
            .unwrap();
        Client { inner, redirects }
    }

    pub fn new_follow() -> Self {
        let redirects = sync::Arc::new(sync::Mutex::new(vec![]));
        let inner = reqwest::blocking::Client::builder()
            .user_agent("linky")
            .build()
            .unwrap();

        Client { inner, redirects }
    }

    pub fn get<U: reqwest::IntoUrl>(
        &self,
        url: U,
    ) -> reqwest::Result<(
        reqwest::blocking::Response,
        Vec<(reqwest::StatusCode, reqwest::Url)>,
    )> {
        self.redirects.lock().unwrap().clear();
        let response = self.inner.get(url).send()?;
        let redirects = self.redirects.lock().unwrap().clone();
        Ok((response, redirects))
    }

    pub fn fetch_link<'a>(
        &self,
        urldecode: bool,
        link: &Link,
    ) -> result::Result<Document<'a>, sync::Arc<Error>> {
        match *link {
            Link::Path(ref path) => Self::fetch_local(path.as_ref(), urldecode),
            Link::Url(ref url) => self.fetch_remote(url),
        }
        .map_err(|err| sync::Arc::new(err.context(Cow::from(format!("link = {link}")))))
    }

    fn fetch_local<'b>(path: &Path, urldecode: bool) -> Result<Document<'b>> {
        if path.is_relative() {
            Err(Tag::Absolute.as_error())
        } else if path.is_dir() {
            Err(Tag::Directory.as_error())
        } else {
            let reader = File::open(path).or_else(|e| {
                if urldecode {
                    urlencoding::decode(path.to_str().unwrap())
                        .map_err(|_| e)
                        .and_then(File::open)
                } else {
                    Err(e)
                }
            })?;
            Document::parse(reader, &MARKDOWN_CONTENT_TYPE)
        }
    }

    fn fetch_remote<'b>(&self, url: &Url) -> Result<Document<'b>> {
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(Tag::Protocol.as_error());
        }

        let (response, redirects) = self.get(url.as_str())?;

        if !response.status().is_success() {
            return Err(Tag::HttpStatus(response.status()).as_error());
        }
        if !redirects.is_empty() {
            let mut err: Error = Tag::HttpStatus(redirects[0].0).as_error();
            for &(status, ref url) in redirects.iter().rev() {
                err = err.context(Cow::from(format!(
                    "redirect({}) = {}",
                    status.as_u16(),
                    url
                )));
            }
            return Err(err);
        }
        let content_type: Result<HeaderValue> = response
            .headers()
            .get(CONTENT_TYPE)
            .cloned()
            .ok_or_else(|| Tag::NoMime.as_error());
        let content_type: mime::Mime = content_type?.to_str()?.parse()?;

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

struct MdAnchorParser<'a> {
    parser: Parser<'a>,
    is_header: bool,
    headers: &'a mut Headers,
    id_transform: &'a dyn ToId,
    header_acc: String,
}

impl<'a> MdAnchorParser<'a> {
    fn new(parser: Parser<'a>, id_transform: &'a dyn ToId, headers: &'a mut Headers) -> Self {
        MdAnchorParser {
            parser,
            is_header: false,
            headers,
            id_transform,
            header_acc: String::new(),
        }
    }

    fn from_buffer(buffer: &'a str, id_transform: &'a dyn ToId, headers: &'a mut Headers) -> Self {
        MdAnchorParser::new(Parser::new(buffer), id_transform, headers)
    }
}

impl<'a> Iterator for MdAnchorParser<'a> {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        for event in self.parser.by_ref() {
            match event {
                Event::Start(pulldown_cmark::Tag::Heading(_)) => {
                    self.is_header = true;
                }
                Event::Text(text) => {
                    if self.is_header {
                        self.header_acc.push_str(text.to_string().as_str());
                    }
                }
                Event::Code(text) => {
                    if self.is_header {
                        self.header_acc.push('`');
                        self.header_acc.push_str(text.to_string().as_str());
                        self.header_acc.push('`');
                    }
                }
                Event::End(pulldown_cmark::Tag::Heading(_)) => {
                    self.is_header = false;
                    let count = self.headers.register(self.header_acc.clone());
                    let result = Some(self.id_transform.to_id(self.header_acc.as_ref(), count));
                    self.header_acc.clear();
                    return result;
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

fn read_chars(reader: &mut dyn Read, charset_hint: Option<String>) -> Result<String> {
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

pub fn slurp<P: AsRef<Path>>(filename: &P, buffer: &mut String) -> io::Result<usize> {
    File::open(filename.as_ref())?.read_to_string(buffer)
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
        let text = text.replace(' ', "-");
        if repetition == 0 {
            text
        } else {
            format!("{text}-{repetition}")
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
    parser: OffsetIter<'a>,
    linenum: usize,
    oldoffs: usize,
}

impl<'a> MdLinkParser<'a> {
    pub fn new(buffer: &'a str) -> Self {
        MdLinkParser {
            parser: Parser::new(buffer).into_offset_iter(),
            buffer,
            linenum: 1,
            oldoffs: 0,
        }
    }
}

impl<'a> Iterator for MdLinkParser<'a> {
    type Item = (usize, CowStr<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        for (event, range) in self.parser.by_ref() {
            let offset = range.end;
            if let Event::Start(pulldown_cmark::Tag::Link(_, url, _)) = event {
                self.linenum += count(&self.buffer.as_bytes()[self.oldoffs..offset], b'\n');
                self.oldoffs = offset;
                return Some((self.linenum, url));
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct Record {
    pub doc_path: PathBuf,
    pub doc_line: usize,
    pub link: String,
}

impl Record {
    pub fn to_link<T: AsRef<Path>>(
        &self,
        base_path: &Option<T>,
    ) -> result::Result<(Link, Option<String>), url::ParseError> {
        match Url::parse(&self.link) {
            Ok(url) => Ok(Link::from_url(url)),
            Err(url::ParseError::RelativeUrlWithoutBase) => Link::path(
                &self.link,
                &fs::canonicalize(&self.doc_path).unwrap(),
                base_path,
            ),
            Err(err) => Err(err),
        }
    }
}

lazy_static! {
    static ref RECORD_REGEX: Regex = Regex::new(r"^(.*):(\d+): [^ ]* ([^ ]*)$").unwrap();
}

impl FromStr for Record {
    type Err = &'static str;
    fn from_str(line: &str) -> result::Result<Self, Self::Err> {
        let cap = RECORD_REGEX.captures(line).ok_or("invalid record format")?;
        Ok(Record {
            doc_path: cap.get(1).unwrap().as_str().into(),
            doc_line: cap.get(2).unwrap().as_str().parse().unwrap(),
            link: cap.get(3).unwrap().as_str().to_string(),
        })
    }
}

pub fn read_md(path: &str) -> result::Result<Box<dyn Iterator<Item = Record>>, io::Error> {
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    let parser = MdLinkParser::new(buffer.as_str()).map(|(lineno, url)| Record {
        doc_path: path.into(),
        doc_line: lineno,
        link: url.as_ref().to_string(),
    });
    Ok(Box::new(parser.collect::<Vec<_>>().into_iter()))
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
                3,
                "https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md"
                    .into()
            ))
        );
        assert_eq!(parser.next(), Some((4, "https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing".into())));
        assert_eq!(parser.next(), Some((5, "other.md".into())));
        assert_eq!(parser.next(), Some((6, "non-existing.md".into())));
        assert_eq!(parser.next(), Some((7, "other.md#existing".into())));
        assert_eq!(parser.next(), Some((8, "other.md#non-existing".into())));
        assert_eq!(parser.next(), Some((9, "#heading".into())));
        assert_eq!(parser.next(), Some((10, "#non-existing".into())));
        assert_eq!(parser.next(), Some((11, "#heading-with-code".into())));
        assert_eq!(parser.next(), Some((12, "#HEADING".into())));
        assert_eq!(parser.next(), None);
    }

    #[test]
    fn fragment() {
        assert_eq!(
            FragResolver::new()
                .fragment(&Document::from(&["abc"]), "abc")
                .map_err(|e| e.tag),
            Ok(())
        );
        assert_eq!(
            FragResolver::new()
                .fragment(&Document::new(), "")
                .map_err(|e| e.tag),
            Ok(())
        );
        assert_eq!(
            FragResolver::new()
                .fragment(&Document::empty(), "")
                .map_err(|e| e.tag),
            Err(Tag::NoFragment)
        );
        assert_eq!(
            FragResolver::from(&["prefix"])
                .fragment(&Document::from(&["prefix"]), "")
                .map_err(|e| e.tag),
            Ok(())
        );
        assert_eq!(
            FragResolver::new()
                .fragment(&Document::new(), "abc")
                .map_err(|e| e.tag),
            Err(Tag::NoFragment)
        );
        assert_eq!(
            FragResolver::new()
                .fragment(&Document::from(&["abc-123"]), "123")
                .map_err(|e| e.tag),
            Err(Tag::NoFragment)
        );
        assert_eq!(
            FragResolver::from(&["abc-"])
                .fragment(&Document::from(&["abc-123"]), "123")
                .map_err(|e| e.tag),
            Err(Tag::Prefixed)
        );
    }

    #[test]
    fn find_prefix() {
        assert_eq!(
            FragResolver::new().find_prefix("123", &Document::from(&["123"])),
            Some("")
        );
        assert_eq!(
            FragResolver::from(&["abc-", "def-"]).find_prefix("123", &Document::from(&["def-123"])),
            Some("def-")
        );
        assert_eq!(
            FragResolver::from(&["abc-"]).find_prefix("123", &Document::from(&["def-123"])),
            None
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
