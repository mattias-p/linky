Linky
=====
Extract and check links from Markdown files.

A command line utility to extract links from Markdown files and to check links
for brokenness.


Motivation
----------

Maintaining Markdown documentation you often end up having lots of links to keep up to date.
Linky checks all of the links and singles out the broken ones so you can fix them.

Specifically, linky was created to ease maintenance of Markdown documentation on Github.

It also checks links read from stdin in `grep -Hn` format.


Installation
------------
[Install stable Rust and Cargo]:

```sh
$ curl -sSf https://static.rust-lang.org/rustup.sh | sh
```

Download and unpack source code from the current master branch:

```sh
$ curl https://github.com/mattias-p/linky/archive/master.zip
$ unzip master.zip
$ cd linky-master
```

Compile and link the binary:

```sh
$ cargo build --release
```


Examples
--------

### Inputs

Extract links from Markdown files:

```sh
$ linky examples/single.md examples/examples.md
```

Extract and check links from Markdown files:

```sh
$ linky --check examples/single.md
```

Extract links from Markdown and check the ones containing "README":

```sh
$ linky examples/single.md | grep 'README[^ ]*' | linky --check
```

### Resolution

Resolve absolute local URLs as relative to a local directory:

```sh
$ linky --check --base ./examples/markdown_site examples/examples.md
```

Resolve absolute local URLs as relative to a base domain:

```sh
$ linky --check --base https://github.com/mattias-p/blob/master examples/examples.md
```

Resolve absolute local URLs as relative to a base domain, allowing HTTP redirects:

```sh
$ linky --check --redirect --base http://github.com/mattias-p/blob/master examples/examples.md
```


Link checking
-------------

Invalid URLs are reported, as are links that cannot be resolved.

Absolute path links are either reported immediately or joined to a base URL and resolved.
Relative path links are resolved to readable ordinary files and directories in the local filesystem.
HTTP(S) links are resolved to 200 responses, optionally following redirects.

Links with fragments have their targets parsed so that the fragment can be resolved.
For HTTP(S) links fragments are resolved to HTML anchors.
For relative path links fragments are resolved to Markdown headings.

HTTP(S) links with fragments are always resolved using GET requests.
HTTP(S) links without fragments are resolved using HEAD requests, possibly followed up by a GET request for 405 responses allowing it.

[Install stable Rust and Cargo]: http://doc.crates.io/
