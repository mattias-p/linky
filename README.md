Linky
=====
[![Crates.io]][Crates.io badge]
[![Changelog]][Changelog badge]

Extract links from Markdown files and check links for brokenness.


Motivation
----------

Maintaining Markdown documentation you often end up with lots of links to tend to.
Linky extracts all links and checks them, pointing out the broken ones so you can fix them.
Specifically, linky was created to ease maintenance of Markdown documentation on Github.


Installation
------------

[Install stable Rust and Cargo]:

```sh
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
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
example_site/path/to/example.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:4:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5:  other.md
example_site/path/to/example.md:6:  non-existing.md
example_site/path/to/example.md:7:  other.md#existing
example_site/path/to/example.md:8:  other.md#non-existing
example_site/path/to/example.md:9:  #heading
example_site/path/to/example.md:10:  #non-existing
example_site/path/to/example.md:11:  #heading-with-backticks
```

The output lists all the extracted links along with their respective
input files and line numbers.

Enable the --check option to resolve those links:

```sh
$ linky --check example_site/path/to/example.md
example_site/path/to/example.md:3: OK https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:4: NO_FRAG https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5: OK other.md
example_site/path/to/example.md:6: NO_DOC non-existing.md
example_site/path/to/example.md:7: OK other.md#existing
example_site/path/to/example.md:8: NO_FRAG other.md#non-existing
example_site/path/to/example.md:9: OK #heading
example_site/path/to/example.md:10: NO_FRAG #non-existing
example_site/path/to/example.md:11: OK #heading-with-backticks
```

A status token is now added to each line indicating the outcome of
the resolution.
An `OK` token indicates that the resolution succeeded without remarks.
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
example_site/path/to/example.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:4:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5:  other.md
example_site/path/to/example.md:6:  non-existing.md
example_site/path/to/example.md:7:  other.md#existing
example_site/path/to/example.md:8:  other.md#non-existing
example_site/path/to/example.md:9:  #heading
example_site/path/to/example.md:10:  #non-existing
example_site/path/to/example.md:11:  #heading-with-backticks
example_site/path/to/follow.md:2:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/follow.md:3:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/fragment.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/fragment.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/other.md:2:  example.md
example_site/path/to/transform.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
```

> **Note:** In case your paths contain spaces you may need the find -print0 and xargs -0 options.


### Absolute local links

By default linky doesn't resolve absolute local links.
This way you're able to know which links do redirect.
Also, linky doesn't know the location of the document root.

```sh
$ linky --check example_site/path/to/absolute.md
example_site/path/to/absolute.md:2: ABSOLUTE /path/to/other.md
example_site/path/to/absolute.md:3: ABSOLUTE /path/to/non-existing.md
example_site/path/to/absolute.md:4: ABSOLUTE /path/to/other.md#existing
example_site/path/to/absolute.md:5: ABSOLUTE /path/to/other.md#non-existing
```

If you specify the document root using the --root option linky proceeds
with the resolution relative to that directory:

```sh
$ linky --check --root=example_site example_site/path/to/absolute.md
example_site/path/to/absolute.md:2: OK /path/to/other.md
example_site/path/to/absolute.md:3: NO_DOC /path/to/non-existing.md
example_site/path/to/absolute.md:4: OK /path/to/other.md#existing
example_site/path/to/absolute.md:5: NO_FRAG /path/to/other.md#non-existing
```


### HTTP redirects

By default linky doesn't follow HTTP redirects.
This way you're able to know which links do redirect.

```sh
$ linky --check example_site/path/to/follow.md
example_site/path/to/follow.md:2: HTTP_301 http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/follow.md:3: HTTP_301 http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
```

Enable the --follow option to make linky proceed with the resolution
across HTTP redirects:

```sh
$ linky --check --follow example_site/path/to/follow.md
example_site/path/to/follow.md:2: OK http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/follow.md:3: NO_FRAG http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
```


### URI fragment identifiers

Sometimes when Markdown headings are converted into HTML id attributes a prefix is added to the id attribute.
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


### Transforming links before resolution

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

> **Note:** You may need to be careful with your sed expressions so you don't inadvertently transform the path prefixes.

Finally, pipe the edited linky output into another linky process that actually checks the links:

```sh
$ linky example_site/path/to/transform.md | sed 's,/master/,/example/,' | linky --check
example_site/path/to/transform.md:2: HTTP_404 https://github.com/mattias-p/linky/blob/example/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3: OK https://github.com/mattias-p/linky/blob/example/example_site/path/to/only-on-example-branch.md
```


### Resolution speed

Linky uses a thread pool for link resolution.
The size of the thread pool affects throughput, but the output itself
remains unaffected.

You set the size of the thread pool using the `RAYON_NUM_THREADS`
environment variable:

```sh
$ env RAYON_NUM_THREADS=16 linky --check example_site/path/to/example.md
```

The default size of the thead pool is equal to the number of logical
CPU cores available to the process.
You may be able to achieve much higher throughputs with a larger than
default thread pool.
Use the `time` command to benchmark a suitable size.


### Resolution details

In case you ever wonder why a certain link resolved to whatever status token it got,
set the `RUST_LOG` environment variable to `warn`.

```sh
$ env RUST_LOG=warn linky --check example_site/path/to/example.md
example_site/path/to/example.md:3: OK https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
 WARN  linky > Fragment not found
 WARN  linky >   context: link = https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
 WARN  linky >   context: fragment = #existing
example_site/path/to/example.md:4: NO_FRAG https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5: OK other.md
 WARN  linky > Document not found
 WARN  linky >   context: link = /tmp/linky/example_site/path/to/non-existing.md
 WARN  linky >   caused by: No such file or directory (os error 2)
example_site/path/to/example.md:6: NO_DOC non-existing.md
example_site/path/to/example.md:7: OK other.md#existing
 WARN  linky > Fragment not found
 WARN  linky >   context: link = /tmp/linky/example_site/path/to/other.md
 WARN  linky >   context: fragment = #non-existing
example_site/path/to/example.md:8: NO_FRAG other.md#non-existing
example_site/path/to/example.md:9: OK #heading
 WARN  linky > Fragment not found
 WARN  linky >   context: link = /tmp/linky/example_site/path/to/example.md
 WARN  linky >   context: fragment = #non-existing
example_site/path/to/example.md:10: NO_FRAG #non-existing
example_site/path/to/example.md:11: OK #heading-with-backticks
```


Link resolution
---------------

Local links are resolved to readable ordinary files and directories in the local filesystem.
HTTP(S) links are resolved using GET requests to 200-responses, optionally following redirects.
Target documents are read and decoded into character strings.

For HTTP(S) links fragments are resolved to HTML anchors.
For local links fragments are resolved to Markdown headings.
Fragment resolution is attempted first without any prefix and then,
if that fails, with each of the prefixes, if any were provided.


Issues
------

In case you experience an issue using `linky`, it would be most helpful
if you provide a test Markdown document and the verbose output of the
execution.

For example:

```sh
$ env RUST_LOG=debug RUST_BACKTRACE=1 linky --check  test.md 2&> linky_err.log
```

> **Note:** `RUST_LOG` controls the logging verbosity.
> `RUST_BACKTRACE` controls the printing of the stack trace on panic.

Simply drag-and-drop the resulting `linky_err.log` file into the issue
editor of Github.
Unfortunately, as of February 2018, Github does not allow to drag-and-drop
of Markdown files (\*.md). You can either:

- rename your `test.md` file to `test.md.txt`
- use a third party paste service (e.g. [Hastebin](https://hastebin.com/))
- if the file is not large, inline it into the issue inside code blocks:
  
  ´´´Markdown
  
  your test markdown here ...
  
  ´´´


License
-------

Copyright 2017-2019 Mattias Päivärinta

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
[Changelog badge]: https://github.com/mattias-p/linky/blob/master/CHANGELOG.md
[Changelog]: https://raw.githubusercontent.com/mattias-p/linky/master/img/changelog.svg?sanitize=true
[Crates.io badge]: https://crates.io/crates/linky
[Crates.io]: https://img.shields.io/crates/v/linky.svg
[Install stable Rust and Cargo]: https://www.rust-lang.org/en-US/install.html
[Link resolution section]: #link-resolution
