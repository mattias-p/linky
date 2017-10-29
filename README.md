Linky
=====
A link extraction and checking utility.


Usage
-----
Extract links from Markdown files:

    linky first.md second.md third.md

Extract and check links from Markdown files:

    linky -c first.md second.md third.md

Extract links from Markdown and check the ones containing "README":

    linky first.md second.md third.md | grep README | linky -c
