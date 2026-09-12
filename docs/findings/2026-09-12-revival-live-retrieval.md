# Plan 2 live retrieval verification

The isolated service probe passed all seven checks using the Plan2 debug binary on2026-09-12. It creates a temporary project and JULIE_HOME, disables semantic embeddings for deterministic retrieval checks, and stops only its own service. The native semantic comparison is recorded separately in [the paired content measurement](2026-09-12-revival-content-measurement.md).

Run after building the candidate:

```bash
python3 scripts/verification/plan2_retrieval_probe.py /absolute/path/to/julie-server /absolute/path/to/result.json
```

Verified behavior:

- Three distinct calls on one line remain three exact reference sites across independent limit1 pages.
- Both public body tools reassemble the indexed canonical UTF-8/CRLF declaration exactly from source-bound pages. Text output carries each requested page; structured output preserves exact bytes.
- Streamable HTTP MCP and both named CLI commands return the same selected body page as the JSON API.
- Both tools bound default bodies to100lines and return a complete declaration exceeding60kbytes through an explicit window, including text output.
- Both tools reject previous continuations after the declaration is renamed and the workspace reindexed.

## Existing indexing boundary

Files containing a line longer than20,000characters are classified as text-only by the existing minified-source policy. They have searchable text but no canonical extracted symbol body. Plan2 preserves that policy; it does not claim body retrieval for files without canonical symbols. The live large-body fixture uses normal-length lines. A focused formatter regression separately verifies that an existing canonical60kbyte atomic line is not truncated by an explicit body window.

This probe establishes the public retrieval contracts. It does not qualify every extractor, full Miller replacement, installation packaging, or Windows native semantic sidecar distribution.
