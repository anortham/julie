# Plan 2 scoped content paired measurement

## Inputs

- Frozen corpus: clean detached `6bc1fbde0b83a6a99020198bd1d4b83d804fab81`
- Checkout store: workspace `julie_e818c067`, 8,178/8,178 eligible vectors before freezing
- Initial `facts.sqlite` SHA-256 in both copies: `011e2656a31fed1cbde57a73699ea4d9d1297bc03994327501df722e251ee058`
- Model copy SHA-256: `bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65`
- Sidecar: `/home/murphy/source/julie-semantic-sidecar/target/release/julie-semantic-sidecar`, version 0.1.0
- Before product: detached `b60f7d8e3ca6dbdf921dcac87de3b74c8e06bc2d` plus only the three-file measurement adapter/fixture overlay, patch SHA-256 `76ee88c58a1e5f44f1750fba29523787205540f4c2b5dd8f110d84a2cff1eece`
- Before evaluator SHA-256: `14e40c1779fe866b376c8a35b6827ff67b44b28c971f1e66c4f6ab9dbc0fde62`
- Candidate product: clean `13249765bc1424504cf90a0d847e41ae8e05c4dd`
- Candidate evaluator SHA-256: `113723e3a32fb52c80acc37accb0aa811b7f8064f297b08ed3337cccaa1fe189`

The two evaluators started from byte-identical copies of the stopped-service registry and checkout store. Both used the same immutable corpus and copied model. No live registry, index, service, or model cache was used.

## Result

Both reports contain all nine `revival_scoped_content` rows and zero search errors. In both reports, all three explicit semantic controls report `effective_backend=semantic` and `strategy_id=fast_search_semantic`; all three lexical controls report lexical execution. This proves the native provider and full vector store were active instead of silently degrading to lexical.

The source-comment auto case demonstrates the change. Before, it returned four semantic symbol hits with `content_enriched=false`. After, it returned a full first page of ten mixed hits with `content_enriched=true`, retaining semantic execution while adding scoped line evidence. The Markdown and TOML auto cases use lexical execution in both versions because those scopes have no semantic symbol candidates; both still return their expected file on page one. Explicit semantic controls for those two content-only scopes correctly return zero hits in both versions, while explicit lexical controls return the expected files.

Artifacts:

- `.razorback/sdd/revival-retrieval/matrix-measurement/run/revival-content-before-v2.json`
- `.razorback/sdd/revival-retrieval/matrix-measurement/run/revival-content-after-v2.json`
- `.razorback/sdd/revival-retrieval/matrix-measurement/run/evaluator-before-v2.log`
- `.razorback/sdd/revival-retrieval/matrix-measurement/run/evaluator-after-v2.log`

The earlier `revival-content-before.json` and `revival-content-after.json` are invalid diagnostics: the first adapter omitted provider injection and filtered three TOML cases. They are retained only to document how those harness defects were found.
