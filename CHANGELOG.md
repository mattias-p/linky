# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4]
### Changed
- Resolve links using a thread pool.
- Improved log messages

### Added
- Support for `RAYON_NUM_THREADS` environment variable, controlling the
  new thread pool.

## [0.1.3] - 2018-03-16
### Fixed
- Resolve local directories to status DIR [#16](https://github.com/mattias-p/linky/issues/16)
- Resolve remote xml files to OK instead of MIME [#17](https://github.com/mattias-p/linky/issues/17)
- Resolve remote pdf files to OK instead of DEC\_ERR [#18](https://github.com/mattias-p/linky/issues/18)

## [0.1.2] - 2018-03-04
### Fixed
- Detect and decode non-UTF-8 encodings [#14](https://github.com/mattias-p/linky/issues/14)
- Tag empty fragments with OK instead of NO\_FRAG
- Added issues section to README by [@bugabinga]
- Updated dependency on bytecount to 0.3.1 by [@llogiq]

## [0.1.1] - 2017-11-25
### Changed
- Updated installation instruction in README

## [0.1.0] - 2017-11-25
### Added
- Initial version


[@bugabinga]: https://github.com/bugabinga/
[@llogiq]: https://github.com/llogiq/
