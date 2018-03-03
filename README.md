Linky
=====
[![Crates.io](https://img.shields.io/crates/v/linky.svg)](https://crates.io/crates/linky)

Extract and check links from Markdown files.

A command line utility to extract links from Markdown files and to check links
for brokenness.


Motivation
----------

Maintaining Markdown documentation you often end up with lots of links to tend to.
Linky extracts all links and checks them, pointing out the broken ones so you can fix them.
Specifically, linky was created to ease maintenance of Markdown documentation on Github.


Installation
------------

[Install stable Rust and Cargo]:

```sh
$ curl https://sh.rustup.rs -sSf | sh
```

Compile and install linky using Cargo:

```sh
$ cargo install linky
```


Usage
-----

### Extracting and checking links

The simplest thing you can do with linky is to extract links from a Markdown file:

```sh
$ linky example_site/path/to/example.md
example_site/path/to/example.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:4:  other.md
example_site/path/to/example.md:5:  non-existing.md
example_site/path/to/example.md:6:  other.md#existing
example_site/path/to/example.md:7:  other.md#non-existing
example_site/path/to/example.md:8:  #heading
example_site/path/to/example.md:9:  #non-existing
```

Each line in the output presents a link and where it was found, with source file path and line numbers.

To check which links are broken and in what way, just add the --check option:

```sh
$ linky --check example_site/path/to/example.md
example_site/path/to/example.md:2: OK https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:3: NO_FRAG https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:4: OK other.md
example_site/path/to/example.md:5: NO_DOC non-existing.md
example_site/path/to/example.md:6: OK other.md#existing
example_site/path/to/example.md:7: NO_FRAG other.md#non-existing
example_site/path/to/example.md:8: OK #heading
example_site/path/to/example.md:9: NO_FRAG #non-existing
```

With --check a status token is added to each line.
`OK` means that resolution succeeded.
Other tokens represent different kinds of failure.
For details on how links are resolved see the [link resolution section].


### Recursive directory traversal

Linky doesn't do directory traversal on its own.
Instead it integrates well with find and xargs:

```sh
$ find example_site -type f | xargs linky
example_site/path/to/absolute.md:2:  /path/to/other.md
example_site/path/to/absolute.md:3:  /path/to/non-existing.md
example_site/path/to/absolute.md:4:  /path/to/other.md#existing
example_site/path/to/absolute.md:5:  /path/to/other.md#non-existing
example_site/path/to/example.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:4:  other.md
example_site/path/to/example.md:5:  non-existing.md
example_site/path/to/example.md:6:  other.md#existing
example_site/path/to/example.md:7:  other.md#non-existing
example_site/path/to/example.md:8:  #heading
example_site/path/to/example.md:9:  #non-existing
example_site/path/to/follow.md:2:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/follow.md:3:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/fragment.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/fragment.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/other.md:2:  example.md
example_site/path/to/transform.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
```

> **Note:** In case your paths contain spaces you may need the find -print0 and xargs -0 options.


### Dealing with absolute local links

By default linky doesn't resolve absolute local links:

```sh
$ linky --check example_site/path/to/absolute.md
example_site/path/to/absolute.md:2: ABSOLUTE /path/to/other.md
example_site/path/to/absolute.md:3: ABSOLUTE /path/to/non-existing.md
example_site/path/to/absolute.md:4: ABSOLUTE /path/to/other.md#existing
example_site/path/to/absolute.md:5: ABSOLUTE /path/to/other.md#non-existing
```

Specify a document root using --root to resolve absolute local links
relative to that document root:

```sh
$ linky --check --root=example_site example_site/path/to/absolute.md
example_site/path/to/absolute.md:2: OK /path/to/other.md
example_site/path/to/absolute.md:3: NO_DOC /path/to/non-existing.md
example_site/path/to/absolute.md:4: OK /path/to/other.md#existing
example_site/path/to/absolute.md:5: NO_FRAG /path/to/other.md#non-existing
```

With --root the ABSOLUTE status tokens have been replaced with more detailed ones.


### Dealing with HTTP redirects

By default linky doesn't follow HTTP redirects.
This way you're able to know which links do redirect.

```sh
$ linky --check example_site/path/to/follow.md
example_site/path/to/follow.md:2: HTTP_301 http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/follow.md:3: HTTP_301 http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
```

Enable the --follow option to follow HTTP redirects when resolving links:

```sh
$ linky --check --follow example_site/path/to/follow.md
example_site/path/to/follow.md:2: OK http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/follow.md:3: NO_FRAG http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
```

With --follow the HTTP\_301 status tokens have been replaced with more detailed ones.


### Dealing with HTTP fragments

Sometimes the transformation from headings into HTML id attributes involves adding a prefix to the resulting id attribute.
E.g. Github adds a "user-content-" prefix.

First, let's just try out the example file without specifying a prefix:

```sh
$ linky --check example_site/path/to/fragment.md
example_site/path/to/fragment.md:2: NO_FRAG https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/fragment.md:3: NO_FRAG https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
```

Now, let's try adding that prefix:

```sh
$ linky --check --prefix='user-content-' example_site/path/to/fragment.md
example_site/path/to/fragment.md:2: PREFIXED https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/fragment.md:3: NO_FRAG https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
```


### Custom link transformation prior to resolution

If you, for example, want to check links against a development version of a sister site you can pipe your links through sed to transform the base URL.

First, let's just extract all links from the example file:

```sh
$ linky example_site/path/to/transform.md
example_site/path/to/transform.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
```

Use sed to edit the links so they point to the sister site:

```sh
$ linky example_site/path/to/transform.md | sed 's,/master/,/example/,'
example_site/path/to/transform.md:2:  https://github.com/mattias-p/linky/blob/example/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3:  https://github.com/mattias-p/linky/blob/example/example_site/path/to/only-on-example-branch.md
```

> **Note:** You may need to be careful with your sed expressoins so you don't inadvertently transform the path prefixes.

Finally, pipe the edited linky output into another linky process that actually checks the links:

```sh
$ linky example_site/path/to/transform.md | sed 's,/master/,/example/,' | linky --check
example_site/path/to/transform.md:2: HTTP_404 https://github.com/mattias-p/linky/blob/example/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3: OK https://github.com/mattias-p/linky/blob/example/example_site/path/to/only-on-example-branch.md
```


### Resolution details

In case you ever wonder why a certain link resolved to whatever status token it got,
set the `RUST_LOG` environment variable to `warn`.

```sh
$ env RUST_LOG=warn linky --check example_site/path/to/example.md
example_site/path/to/example.md:2: OK https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
WARN :linky::linky: warn: Link: https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
WARN :linky::linky:   caused by: Fragment: existing
WARN :linky::linky:   caused by: Fragment not found
example_site/path/to/example.md:3: NO_FRAG https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:4: OK other.md
WARN :linky::linky: warn: Link: example_site/path/to/non-existing.md
WARN :linky::linky:   caused by: Document not found
WARN :linky::linky:   caused by: No such file or directory (os error 2)
example_site/path/to/example.md:5: NO_DOC non-existing.md
example_site/path/to/example.md:6: OK other.md#existing
WARN :linky::linky: warn: Link: example_site/path/to/other.md
WARN :linky::linky:   caused by: Fragment: non-existing
WARN :linky::linky:   caused by: Fragment not found
example_site/path/to/example.md:7: NO_FRAG other.md#non-existing
example_site/path/to/example.md:8: OK #heading
WARN :linky::linky: warn: Link: example_site/path/to/example.md
WARN :linky::linky:   caused by: Fragment: non-existing
WARN :linky::linky:   caused by: Fragment not found
example_site/path/to/example.md:9: NO_FRAG #non-existing
```


Link resolution
---------------

Local links are resolved to readable ordinary files and directories in the local filesystem.
HTTP(S) links are resolved using GET requests to 200-responses, optionally following redirects.

For links with fragments the target documents are read, as opposed to just being checked for existence.
For HTTP(S) links fragments are resolved to HTML anchors.
For local links fragments are resolved to Markdown headings.

When one or more prefixes are provided and an HTTP(S) link fragment cannot be resolved,
resolution is attempted using the frgment prefixed by each of the provided prefixes.


License
-------

Copyright 2017 Mattias Päivärinta

Licensed under the [Apache License, Version 2.0] (the "License");
you may not use any of the files in this distribution except in compliance with
the License. You may obtain a copy of the License at

<http://www.apache.org/licenses/LICENSE-2.0>

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.


[Apache License, Version 2.0]: LICENSE
[Install stable Rust and Cargo]: http://doc.crates.io/
[Link resolution section]: #link-resolution
