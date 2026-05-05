# Extraction Golden Fixtures

These fixtures are the parser-upgrade gate for `julie-extractors`.

Each language registry entry has one or more cases in `capabilities.json`.
Every case has:

- `source`: source input passed through `extract_canonical`
- `expected`: normalized extraction output written by the golden harness

Run normal verification with:

```bash
cargo nextest run -p julie-extractors golden
cargo nextest run -p julie-extractors capability_matrix
```

Regenerate expected files only when the extractor output change is intentional:

```bash
UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden
```

Do not add a language to the registry without adding a matrix row and fixture.
Variants count as separate entries, including `tsx`, `jsx`, and `vue`.
