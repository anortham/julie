# Reference-site language ledger

Julie pins `julie-extractors` v2.42.0. The pinned
[capability registry](https://github.com/anortham/julie-extractors/blob/v2.42.0/fixtures/extraction/capabilities.json)
contains 40 language rows. Its `identifiers` and `relationships` flags are the
applicability source below. The upstream
[fact types](https://github.com/anortham/julie-extractors/blob/v2.42.0/crates/julie-extractors/src/base/types.rs#L293-L403)
define stable identifier IDs and full spans, plus optional relationship spans,
`reference_site_is_exact`, confidence, and metadata.

All exposed spans keep the canonical coordinate convention: lines are 1-based,
columns are 0-based, and byte offsets address the original UTF-8 source.

| Language row | Identifier sites | Relationship sites | Julie precision contract |
|---|---:|---:|---|
| rust | yes | yes | exact identifier; relationship exactness retained |
| c | yes | yes | exact identifier; relationship exactness retained |
| cpp | yes | yes | exact identifier; relationship exactness retained |
| go | yes | yes | exact identifier; relationship exactness retained |
| zig | yes | yes | exact identifier; relationship exactness retained |
| typescript | yes | yes | exact identifier; relationship exactness retained |
| tsx | yes | yes | exact identifier; relationship exactness retained |
| javascript | yes | yes | exact identifier; relationship exactness retained |
| jsx | yes | yes | exact identifier; relationship exactness retained |
| html | yes | yes | exact identifier; relationship exactness retained |
| css | no | yes | relationship span and exactness retained |
| vue | yes | yes | exact identifier; relationship exactness retained |
| python | yes | yes | exact identifier; relationship exactness retained |
| java | yes | yes | exact identifier; relationship exactness retained |
| csharp | yes | yes | exact identifier; relationship exactness retained |
| vbnet | yes | yes | exact identifier; relationship exactness retained |
| php | yes | yes | exact identifier; relationship exactness retained |
| ruby | yes | yes | exact identifier; relationship exactness retained |
| swift | yes | yes | exact identifier; relationship exactness retained |
| kotlin | yes | yes | exact identifier; relationship exactness retained |
| scala | yes | yes | exact identifier; relationship exactness retained |
| dart | yes | yes | exact identifier; relationship exactness retained |
| elixir | yes | yes | exact identifier; relationship exactness retained |
| fsharp | yes | yes | exact identifier; relationship exactness retained |
| erlang | yes | yes | exact identifier; relationship exactness retained |
| lua | yes | yes | exact identifier; relationship exactness retained |
| qml | yes | yes | exact identifier; relationship exactness retained |
| qmldir | no | no | current fast_refs input absent; applicability gap described below |
| r | yes | yes | exact identifier; relationship exactness retained |
| bash | yes | yes | exact identifier; relationship exactness retained |
| powershell | yes | yes | exact identifier; relationship exactness retained |
| gdscript | yes | yes | exact identifier; relationship exactness retained |
| razor | no | yes | relationship span and exactness retained |
| sql | yes | yes | exact identifier; relationship exactness retained |
| regex | no | yes | relationship span and exactness retained |
| markdown | no | yes | relationship span and exactness retained |
| json | no | yes | relationship span and exactness retained |
| toml | no | yes | relationship span and exactness retained |
| yaml | no | yes | relationship span and exactness retained |
| xml | no | yes | relationship span and exactness retained |

`tsx` and `jsx` are registry variants of TypeScript and JavaScript; `qmldir`
is a basename-driven manifest row. They remain separate here because the
capability registry is the canonical inventory.

The qmldir extractor returns no identifier or relationship rows, matching the
registry flags, but it records `depends`, `import`, `optional import`, type
files, and paths as source-spanned structural facts. Those directives can carry
reference-like meaning. The registry exception proves only that they are not
emitted through the identifier/relationship contracts; it does not prove
reference-site navigation is intrinsically inapplicable. Qmldir fast_refs
support remains an upstream modeling and consumer-evidence gap until the
extractor contract classifies these directives as navigable relationships or
positively rules that out.

The registry proves whether each language emits identifiers and relationships,
but it does not certify that every relationship kind has an exact target-token
span. Julie therefore preserves the upstream exactness flag verbatim and labels
identifier, relationship, combined, and graph-fallback provenance. A future
upstream capability domain must enumerate exact and inferred relationship kinds
per language before Julie can claim stronger precision.
