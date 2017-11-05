Linky
=====

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
$ curl -sSf https://static.rust-lang.org/rustup.sh | sh
```

Download and unpack source code from the current master branch:

```sh
$ curl https://github.com/mattias-p/linky/archive/master.zip
$ unzip master.zip
$ cd linky-master
```

Compile and install the binary:

```sh
$ cargo install
```


Usage
-----

### Extracting links

The simplest thing you can do with linky is to extract links from a Markdown file:

```sh
$ linky example_site/path/to/example.md
example_site/path/to/example.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:3:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:4:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/example.md:6:  other.md
example_site/path/to/example.md:7:  non-existing.md
example_site/path/to/example.md:8:  other.md#heading
example_site/path/to/example.md:9:  other.md#non-existing
example_site/path/to/example.md:10:  /path/to/other.md
example_site/path/to/example.md:11:  /path/to/non-existing.md
example_site/path/to/example.md:12:  /path/to/other.md#existing
example_site/path/to/example.md:13:  /path/to/other.md#non-existing
example_site/path/to/example.md:14:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
example_site/path/to/example.md:15:  #heading
example_site/path/to/example.md:16:  #non-existing
```

To extract links from a directory structure we use find and xargs:

```sh
$ find example_site -type f | xargs linky
example_site/path/to/example.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:3:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:4:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5:  http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/example.md:6:  other.md
example_site/path/to/example.md:7:  non-existing.md
example_site/path/to/example.md:8:  other.md#heading
example_site/path/to/example.md:9:  other.md#non-existing
example_site/path/to/example.md:10:  /path/to/other.md
example_site/path/to/example.md:11:  /path/to/non-existing.md
example_site/path/to/example.md:12:  /path/to/other.md#existing
example_site/path/to/example.md:13:  /path/to/other.md#non-existing
example_site/path/to/example.md:14:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
example_site/path/to/example.md:15:  #heading
example_site/path/to/example.md:16:  #non-existing
example_site/path/to/other.md:2:  example.md
```

> **Note:** In case your paths contain spaces you may need the find -print0 and xargs -0 options.

Let's take a look at the output format.
Each line presents a link and where it was found, with source file path and line numbers.


### Checking links

To check which links are broken and in what way, just add the --check option:

```sh
$ linky --check example_site/path/to/example.md
example_site/path/to/example.md:3: HTTP_301 http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md
example_site/path/to/example.md:4: NO_ANCHOR https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5: HTTP_301 http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/example.md:7: NO_DOCUMENT non-existing.md
example_site/path/to/example.md:8: NO_ANCHOR other.md#heading
example_site/path/to/example.md:9: NO_ANCHOR other.md#non-existing
example_site/path/to/example.md:10: ABSOLUTE /path/to/other.md
example_site/path/to/example.md:11: ABSOLUTE /path/to/non-existing.md
example_site/path/to/example.md:12: ABSOLUTE /path/to/other.md#existing
example_site/path/to/example.md:13: ABSOLUTE /path/to/other.md#non-existing
example_site/path/to/example.md:14: HTTP_404 https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
example_site/path/to/example.md:16: NO_ANCHOR #non-existing
```

Notice that fewer lines are printed.
The links that could be successfully resolved were filtered out of the output.
For details on how links are checked see the [link resolution section].

Also notice that an error token has been added to each one of the remaining lines.
This error token indicates how the link resolution failed.


### Dealing with absolute local links

If absolute local links are used, linky can't resolve those without knowing the document root directory.
This is specified using the --root option (which defaults to the file system root).
First, let's just examine the links in the example file:

```sh
$ linky --check example_site/path/to/absolute.md
example_site/path/to/absolute.md:2: ABSOLUTE /path/to/other.md
example_site/path/to/absolute.md:3: ABSOLUTE /path/to/non-existing.md
example_site/path/to/absolute.md:4: ABSOLUTE /path/to/other.md#existing
example_site/path/to/absolute.md:5: ABSOLUTE /path/to/other.md#non-existing
```

Specify the document root directory to let the resolution to continue:

```sh
$ linky --check --root=example_site example_site/path/to/absolute.md
example_site/path/to/absolute.md:3: NO_DOCUMENT /path/to/non-existing.md
example_site/path/to/absolute.md:5: NO_ANCHOR /path/to/other.md#non-existing
```


### Dealing with HTTP redirects

Checking the links in example.md with linky you should see a couple of lines with HTTP\_301 error tokens.
By default linky does not follow HTTP redirects.
If you want HTTP redirects to be followed simply specify the --follow option.

```sh
$ linky --check --follow example_site/path/to/example.md
example_site/path/to/example.md:4: NO_ANCHOR https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing
example_site/path/to/example.md:5: NO_ANCHOR http://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#non-existing
example_site/path/to/example.md:7: NO_DOCUMENT non-existing.md
example_site/path/to/example.md:8: NO_ANCHOR other.md#heading
example_site/path/to/example.md:9: NO_ANCHOR other.md#non-existing
example_site/path/to/example.md:10: ABSOLUTE /path/to/other.md
example_site/path/to/example.md:11: ABSOLUTE /path/to/non-existing.md
example_site/path/to/example.md:12: ABSOLUTE /path/to/other.md#existing
example_site/path/to/example.md:13: ABSOLUTE /path/to/other.md#non-existing
example_site/path/to/example.md:14: HTTP_404 https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
example_site/path/to/example.md:16: NO_ANCHOR #non-existing
```

Notice that the links that previously had HTTP\_301 error tokens now have disappeared or have contracted other resolution problems.


### Custom link transformation prior to resolution

If you, for example, want to check links against a development version of a sister site you can pipe your links through sed to transform the base URL.
First, let's just extract the links from the example file:

```sh
$ linky example_site/path/to/transform.md
example_site/path/to/transform.md:2:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3:  https://github.com/mattias-p/linky/blob/master/example_site/path/to/only-on-example-branch.md
```

Then use sed to edit the links to point to the sister site:

```sh
$ linky example_site/path/to/transform.md | sed 's,/master/,/example/,'
example_site/path/to/transform.md:2:  https://github.com/mattias-p/linky/blob/example/example_site/path/to/non-existing.md
example_site/path/to/transform.md:3:  https://github.com/mattias-p/linky/blob/example/example_site/path/to/only-on-example-branch.md
```

> **Note:** You may need to be careful with your sed expressoins so you don't inadvertently transform the path prefixes.

Finally, pipe the edited linky output into another linky process that actually checks the links:

```sh
$ linky example_site/path/to/transform.md | sed 's,/master/,/example/,' | linky --check
example_site/path/to/transform.md:2: HTTP_404 https://github.com/mattias-p/linky/blob/master/example_site/path/to/non-existing.md
```


Link resolution
---------------

Local links are resolved to readable ordinary files and directories in the local filesystem.
HTTP(S) links are resolved to 200-responses, optionally following redirects.

For links with fragments the target documents are read, as opposed to just being checked for existence.
For HTTP(S) links fragments are resolved to HTML anchors.
For local links fragments are resolved to Markdown headings.

HTTP(S) links with fragments are always resolved using GET requests.
HTTP(S) links without fragments are resolved using HEAD requests, possibly followed up by a GET request for 405 responses allowing it.


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
