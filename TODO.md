# TODO

## Bugs

- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)

## Tech Debt

- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas

## Enhancements

- [ ] **Windows Python launcher `py -3.12` / `py -3.13` probing** — `python_interpreter_candidates()` doesn't try `py -3.12` syntax, which is the standard way to request a specific version on Windows (`src/embeddings/sidecar_bootstrap.rs:197-208`)
