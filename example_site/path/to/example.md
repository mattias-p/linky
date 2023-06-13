# Heading
# Heading with `code`
* [remote link without fragment, ok](https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md)
* [remote link with fragment, ok](https://github.com/mattias-p/linky/blob/master/example_site/path/to/other.md#existing)
* [relative link without fragment, ok](other.md)
* [relative link without fragment, broken](non-existing.md)
* [relative link with fragment, ok](other.md#existing)
* [relative link with fragment, broken](other.md#non-existing)
* [in-document link with fragment, ok](#heading)
* [in-document link with fragment, broken](#non-existing)
* [in-document link to heading with code, ok](#heading-with-code)
* [in-document link with case-insensitive fragment, broken](#HEADING)
