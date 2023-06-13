# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog].
and this project adheres to [Semantic Versioning].

## [Unreleased]
### Fixed
- Fix connection errors with domains resolved to IPv6 addresses.

### Added
- New --link-only flag ([#30], courtesy of [@egrieco]).
- Support links to markdown headings with inline code (backticks).

### Other
- Introduce GitHub Actions for push and pull\_request ([#27]), courtesy of [@sanxiyn].
- Some refactorings to clean up the implementation.

## [0.1.8] - 2019-12-15
### Security
- Remove reliance on dependencies with known vulnerabilities.

### Other
- Clean up in dependencies ([#26]), courtesy of [@sanxiyn].

## [0.1.7] - 2019-12-05
### Changed
- Update dependency on pulldown\_cmark to 0.6.1 from 0.0.11 ([#25]), courtesy of [@sanxiyn].
  - Changes recognized CommonMark version to 0.29 (most likely from 0.27).

## [0.1.6] - 2019-05-31
### Fixed
- Markdown headings with spaces are now properly recognized.

### Added
- Log all found anchors at the debug level.

## [0.1.5] - 2019-05-31
### Fixed
- Fix a bug where links were erroneously resolved to ABSOLUTE.

### Added
- Decode URL-encoded local links in --urldecode mode ([#21]).
- Log redirects in --no-follow mode.

## [0.1.4] - 2018-03-30
### Changed
- Resolve links using a thread pool.
- Improve log messages.

### Added
- Support for `RAYON_NUM_THREADS` environment variable, controlling the
  new thread pool.

## [0.1.3] - 2018-03-16
### Fixed
- Resolve local directories to status DIR ([#16]).
- Resolve remote xml files to OK instead of MIME ([#17]).
- Resolve remote pdf files to OK instead of DEC\_ERR ([#18]).

## [0.1.2] - 2018-03-04
### Fixed
- Detect and decode non-UTF-8 encodings ([#14]).
- Tag empty fragments with OK instead of NO\_FRAG.
- Add issues section to README, courtesy of [@bugabinga].
- Update dependency on bytecount to 0.3.1, courtesy of [@llogiq].

## [0.1.1] - 2017-11-25
### Changed
- Update installation instruction in README.

## [0.1.0] - 2017-11-25
### Added
- Initial version


[@bugabinga]: https://github.com/bugabinga
[@egrieco]: https://github.com/egrieco
[@llogiq]: https://github.com/llogiq
[@sanxiyn]: https://github.com/sanxiyn
[#14]: https://github.com/mattias-p/linky/pull/14
[#16]: https://github.com/mattias-p/linky/pull/16
[#17]: https://github.com/mattias-p/linky/pull/17
[#18]: https://github.com/mattias-p/linky/pull/18
[#21]: https://github.com/mattias-p/linky/issues/21
[#25]: https://github.com/mattias-p/linky/pull/25
[#26]: https://github.com/mattias-p/linky/pull/26
[#27]: https://github.com/mattias-p/linky/pull/27
[#30]: https://github.com/mattias-p/linky/pull/30
[Keep a Changelog]: https://keepachangelog.com/en/1.0.0/
[Semantic Versioning]: https://semver.org/spec/v2.0.0.html
