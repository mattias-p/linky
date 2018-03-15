use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::error;
use std::fmt;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::io;
use std::ops::Add;
use std::path::Path;
use std::rc::Rc;
use std::result;
use std::str::FromStr;

use bytecount::count;
use encoding::label::encoding_from_whatwg_label;
use encoding::DecoderTrap;
use errors::ErrorKind;
use errors::DecodingError;
use errors::FragmentError;
use errors::LinkError;
use errors::LookupError;
use errors::ParseError;
use errors::PrefixError;
use errors::UnrecognizedMime;
use htmlstream;
use pulldown_cmark;
use pulldown_cmark::Event;
use pulldown_cmark::Parser;
use regex::Regex;
use reqwest::Client;
use reqwest::header::ContentType;
use reqwest::mime;
use reqwest::Response;
use url::Url;
use url;
use xhtmlchardet;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Tag(Result<(), ErrorKind>);

impl Tag {
    pub fn ok() -> Self {
        Tag(Ok(()))
    }
    pub fn from_error_kind(kind: ErrorKind) -> Self {
        Tag(Err(kind))
    }
}

impl FromStr for Tag {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "OK" => Ok(Tag(Ok(()))),
            s => Ok(Tag(Err(ErrorKind::from_str(s)?))),
        }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Ok(()) => write!(f, "OK"),
            Err(ref kind) => write!(f, "{}", kind),
        }
    }
}

lazy_static! {
    static ref MARKDOWN_CONTENT_TYPE: ContentType = ContentType("text/markdown; charset=UTF-8"
            .parse::<mime::Mime>()
            .unwrap());
}

enum Format {
    Html,
    Markdown,
}

fn parse<'a, R: Read>(
    mut reader: R,
    content_type: &ContentType,
) -> result::Result<Document<'a>, LookupError> {
    let format = match (content_type.type_(), content_type.subtype().as_str()) {
        (mime::TEXT, "html") => Format::Html,
        (mime::TEXT, "markdown") => Format::Markdown,
        _ => {
            return Ok(Document {
                ids: HashSet::new(),
            });
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
                .map(|id| Cow::from(id))
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

    Ok(Document { ids: ids })
}

pub struct Document<'a> {
    pub ids: HashSet<Cow<'a, str>>,
}

impl<'a> Document<'a> {
    pub fn new() -> Self {
        Document {
            ids: HashSet::new(),
        }
    }

    pub fn from(ids: &'a [&'a str]) -> Self {
        Document {
            ids: ids.iter().cloned().map(Cow::from).collect(),
        }
    }
}

struct FragResolver<'a>(HashSet<&'a str>);

impl<'a> FragResolver<'a> {
    fn fragment<'d>(&self, fragment: &str, document: &Document<'d>) -> Option<&str> {
        if document.ids.contains(&Cow::from(fragment)) {
            return Some("");
        }
        for prefix in &self.0 {
            if document
                .ids
                .contains(format!("{}{}", prefix, fragment).as_str())
            {
                return Some(prefix);
            }
        }
        return None;
    }
}

trait LocalResolver {
    fn local(&self, path: &Path) -> Result<Document, LookupError>;
}

trait RemoteResolver {
    fn remote<'b>(&self, url: &Url) -> Result<Document<'b>, LookupError>;
}

struct FilesystemLocalResolver;

impl LocalResolver for FilesystemLocalResolver {
    fn local(&self, path: &Path) -> Result<Document, LookupError> {
        if path.is_absolute() {
            return Err(ErrorKind::Absolute.into());
        }

        let reader = File::open(&path)?;

        parse(reader, &MARKDOWN_CONTENT_TYPE)
    }
}

struct NetworkRemoteResolver<'a>(&'a Client);

impl<'a> RemoteResolver for NetworkRemoteResolver<'a> {
    fn remote<'b>(&self, url: &Url) -> result::Result<Document<'b>, LookupError> {
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(ErrorKind::Protocol.into());
        }

        let response = self.0.get(url.clone()).send()?;

        if !response.status().is_success() {
            return Err(ErrorKind::HttpStatus(response.status()).into());
        }
        let result = {
            let content_type: Result<ContentType, LookupError> = response
                .headers()
                .get::<ContentType>()
                .map(|ct| ct.clone())
                .ok_or(ErrorKind::NoMime.into());
            let content_type = content_type?;

            parse(response, &content_type)
        };

        result
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
            parser: parser,
            is_header: false,
            headers: headers,
            id_transform: id_transform,
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

fn read_chars(
    reader: &mut Read,
    charset_hint: Option<String>,
) -> result::Result<String, LookupError> {
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    let mut cursor = Cursor::new(buffer);

    let charsets = xhtmlchardet::detect(&mut cursor, charset_hint)?;

    debug!("detected charsets: {:?}", &charsets);

    let charset: result::Result<_, LookupError> = charsets
        .iter()
        .flat_map(|v| encoding_from_whatwg_label(v.as_str()))
        .next()
        .ok_or_else(|| LookupError {
            kind: ErrorKind::DecodingError,
            cause: Some(Box::new(DecodingError::new(
                "Failed to detect character encoding".to_string(),
            ))),
        });

    charset?
        .decode(cursor.into_inner().as_ref(), DecoderTrap::Strict)
        .map_err(|err| LookupError {
            kind: ErrorKind::DecodingError,
            cause: Some(Box::new(DecodingError::new(err.to_string()))),
        })
}

fn slurp<P: AsRef<Path>>(filename: &P, mut buffer: &mut String) -> io::Result<usize> {
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
        match self.0.entry(text.to_string()) {
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

struct MdLinkParser<'a> {
    buffer: &'a str,
    parser: Parser<'a>,
    linenum: usize,
    oldoffs: usize,
}

impl<'a> MdLinkParser<'a> {
    fn new(buffer: &'a str) -> Self {
        MdLinkParser {
            parser: Parser::new(buffer),
            buffer: buffer,
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

pub struct Record {
    pub path: String,
    pub linenum: usize,
    pub link: String,
}

lazy_static! {
    static ref RECORD_REGEX: Regex = Regex::new(r"^(.*):(\d+): [^ ]* ([^ ]*)$").unwrap();
}

impl FromStr for Record {
    type Err = ();
    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let cap = RECORD_REGEX.captures(line).ok_or(())?;
        Ok(Record {
            path: cap.get(1).unwrap().as_str().to_string(),
            linenum: cap.get(2).unwrap().as_str().parse().unwrap(),
            link: cap.get(3).unwrap().as_str().to_string(),
        })
    }
}

pub fn md_file_links<'a>(path: &'a str, links: &mut Vec<Record>) -> io::Result<()> {
    let mut buffer = String::new();
    slurp(&path, &mut buffer)?;
    let parser = MdLinkParser::new(buffer.as_str()).map(|(lineno, url)| Record {
        path: path.to_string(),
        linenum: lineno,
        link: url.as_ref().to_string(),
    });

    links.extend(parser);
    Ok(())
}

fn find_prefixed_fragment(ids: &[&str], fragment: &str, prefixes: &[&str]) -> Option<String> {
    prefixes
        .iter()
        .filter_map(|p| {
            if ids.contains(&format!("{}{}", p, fragment).as_str()) {
                Some(p.to_string())
            } else {
                None
            }
        })
        .next()
}

pub fn lookup_fragment<'a>(
    document: &Document,
    fragment: &str,
    prefixes: &'a [&str],
) -> Result<(), (Tag, FragmentError)> {
    if fragment == "" {
        Ok(())
    } else if document.ids.contains(&Cow::from(fragment)) {
        Ok(())
    } else {
        let ids: Vec<_> = document.ids.iter().map(AsRef::as_ref).collect();
        if let Some(prefix) = find_prefixed_fragment(&ids, fragment, prefixes) {
            let err: LookupError = ErrorKind::Prefixed.into();
            Err((
                Tag::from_error_kind(ErrorKind::Prefixed),
                FragmentError::new(
                    fragment.to_string(),
                    Box::new(PrefixError::new(prefix, Box::new(err))),
                ),
            ))
        } else {
            let err: LookupError = ErrorKind::NoFragment.into();
            Err((
                Tag::from_error_kind(ErrorKind::NoFragment),
                FragmentError::new(fragment.to_string(), Box::new(err)),
            ))
        }
    }
}

pub fn parse_link(record: &Record, root: &str) -> Result<(Link, Option<String>), url::ParseError> {
    Link::parse_with_root(record.link.as_str(), &Path::new(&record.path), &root)
        .map(|parsed| parsed.split_fragment())
}

pub fn resolve_link(
    client: &Client,
    targets: &mut HashMap<Link, Result<Document, (Tag, Rc<LinkError>)>>,
    link: Link,
    fragment: Option<String>,
    prefixes: &[&str],
) -> (Tag, Option<Rc<LinkError>>) {
    targets
        .entry(link.clone())
        .or_insert_with(|| {
            match &link {
                &Link::Path(ref path) => FilesystemLocalResolver.local(path.as_ref()),
                &Link::Url(ref url) => NetworkRemoteResolver(&client).remote(url),
            }.map_err(|err| {
                let tag = Tag::from_error_kind(err.kind());
                (tag, Rc::new(LinkError::new(link.clone(), Box::new(err))))
            })
        })
        .as_ref()
        .map_err(|&(ref tag, ref err)| (tag.clone(), Some(err.clone())))
        .and_then(|document| {
            if let Some(ref fragment) = fragment {
                lookup_fragment(&document, &fragment, prefixes).map_err(|(tag, err)| {
                    (
                        tag.clone(),
                        Some(Rc::new(LinkError::new(link, Box::new(err)))),
                    )
                })
            } else {
                Ok(())
            }
        })
        .err()
        .unwrap_or_else(|| (Tag::ok(), None))
}

pub fn print_warning(err: &error::Error) {
    warn!("warn: {}", &err);
    let mut e = err.cause();
    while let Some(err) = e {
        warn!("  caused by: {}", &err);
        e = err.cause();
    }
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
            lookup_fragment(&Document::from(&["abc"]), "abc", &[]).map_err(|e| e.0),
            Ok(())
        );
        assert_eq!(
            lookup_fragment(&Document::new(), "", &[]).map_err(|e| e.0),
            Ok(())
        );
        assert_eq!(
            lookup_fragment(&Document::from(&["prefix"]), "", &["prefix"]).map_err(|e| e.0),
            Ok(())
        );
        assert_eq!(
            lookup_fragment(&Document::new(), "abc", &[]).map_err(|e| e.0),
            Err(Tag::from_error_kind(ErrorKind::NoFragment))
        );
        assert_eq!(
            lookup_fragment(&Document::from(&["abc-123"]), "123", &["abc-"]).map_err(|e| e.0),
            Err(Tag::from_error_kind(ErrorKind::Prefixed))
        );
    }

    #[test]
    fn find_fragments() {
        assert_eq!(find_prefixed_fragment(&[], "123", &[]), None);
        assert_eq!(find_prefixed_fragment(&["abc-123"], "123", &[]), None);
        assert_eq!(find_prefixed_fragment(&["abc-123"], "123", &["def-"]), None);
        assert_eq!(
            find_prefixed_fragment(&["abc-123"], "123", &["abc-"]),
            Some("abc-".to_string())
        );
        assert_eq!(
            find_prefixed_fragment(&["def-123"], "123", &["abc-", "def-"]),
            Some("def-".to_string())
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
