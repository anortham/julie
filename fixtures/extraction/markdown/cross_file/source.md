# Cross-file references in Markdown

Phase 4d fixture: Markdown links and footnotes are intra-document references
or opaque URL/path strings. There is no symbol-level forward-reference
construct; structured pending is intentionally empty.

See [the other doc](./other.md) for context. See also [external][ext-ref].

[Local heading](#local-heading)

## Local heading

Footnote test[^1] resolves within this document.

[ext-ref]: https://example.com/external "External"
[^1]: An intra-document footnote.
