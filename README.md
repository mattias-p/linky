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
linky example_site/path/to/example.md
```

To extract links from a directory structure we use find and xargs:

```sh
find examples -type f | xargs linky
```

> **Note:** In case your paths contain spaces you may need the find -print0 and xargs -0 options.

Let's take a look at the output format.
Each line presents a link and where it was found, with source file path and line numbers.


### Checking links

To check which links are broken and in what way, just add the --check option:

```sh
linky --check example_site/path/to/example.md
```

Notice that fewer lines are printed.
The links that could be successfully resolved were filtered out of the output.
For details on how links are checked see the [link resolution section].

Also notice that an error token has been added to each one of the remaining lines.
This error token indicates how the link resolution failed.


### Dealing with absolute local links

Checking the links in examples.md with linky you should see a couple of lines with ABSOLUTE error tokens.
Linky can't resolve those links because the document root isn't at the file system root.
We need to override that with the --root option.
As a first step, let's just take a quick look at the --root transformation in isolation:

```sh
linky --root=example_site example_site/path/to/example.md
```

The absolute local links are no longer printed exactly as they were in the source document.
Now, check the links with specified --root:

```sh
linky --check --root=example_site example_site/path/to/example.md
```

Notice that even more lines have disappeared from the output.


### Dealing with HTTP redirects

Checking the links in examples.md with linky you should see a couple of lines with HTTP_301 error tokens.
By default linky does not follow HTTP redirects.
If you want HTTP redirects to be followed simply specify the --redirects option.

```sh
linky --check --redirects example_site/path/to/examples.md
```

Notice that the links that previously had HTTP_301 error tokens now have disappeared or have contracted other resolution problems. 


### Custom link transformation prior to resolution

If you, for example, want to check links against a development version of a sister site you can pipe your links through sed to transform the base URL.

```sh
linky example_site/path/to/examples.md | sed 's,/master/,/develop/,' | linky --check
```

> **Note:** You may need to be careful with your sed expressoins so you don't inadvertently transform the path prefixes.


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
