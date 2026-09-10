# Model Scorecard Finding

**Date:** 2026-09-10
**Branch:** `semantics` at `/home/murphy/source/julie/.worktrees/semantics`
**Commit:** `36a558c0`
**Verdict:** Keep `bge-small-en-v1.5-f32` as the default embedding model (`DEFAULT_NATIVE_MODEL = "bge-small-en-v1.5-f32"`). Under the decision rule ("keep bge-small unless qwen3 wins by more than one top-5 case"), `bge-small-en-v1.5-f32` decisively outperforms `qwen3-0.6b-f16` across all accuracy, latency, memory, and resource dimensions.

---

## 1. Executive Summary

Evaluation was executed across 10 public repositories with 23 evaluation cases defined in `docs/eval/semantic-value/scorecard.toml`.

- **Pure Semantic Search Top-5:** `bge-small-en-v1.5-f32` achieved **91.3% (21/23)** vs `qwen3-0.6b-f16`'s **26.1% (6/23)**.
- **Hybrid Search Top-5:** `bge-small-en-v1.5-f32` achieved **82.6% (19/23)** vs `qwen3-0.6b-f16`'s **78.3% (18/23)**.
- **Memory Footprint:** `bge-small` sidecar child peaked at **357.4 MB** VmRSS vs `qwen3`'s **4,759.2 MB** VmRSS (>13x memory reduction).
- **Inference Latency:** `bge-small` p95 query latency was **3,426 ms** vs `qwen3`'s **5,353 ms**; batch inference on CPU took **~0.1s** per 50 symbols for `bge-small` vs **~8.0s** for `qwen3`.
- **Model Size:** `bge-small` GGUF is **133.6 MB** vs `qwen3`'s **1.20 GB** (9x smaller on disk and download).

`qwen3-0.6b-f16` failed to outperform `bge-small` on any top-k accuracy metric or resource metric. Therefore, `bge-small-en-v1.5-f32` is retained as the default model.

---

## 2. Evaluation Metrics

Evaluations were conducted on Linux using the release binary (`target/release/julie-server`) and `julie-semantic-sidecar` with scratch `JULIE_HOME` directories.

| Metric | Lexical Baseline | `bge-small-en-v1.5-f32` | `qwen3-0.6b-f16` |
|---|---|---|---|
| **Tier in Sidecar** | — | `Tier::Default` | `Tier::Fallback` |
| **Model GGUF Size** | — | **133.6 MB** (`133,609,568` B) | 1,197.6 MB (`1,197,629,632` B) |
| **Native / Serve Dims** | — | 384 / 384 | 1024 / 512 |
| **Model Prepare Time** | — | **0.078 s** | 0.650 s |
| **Child Peak VmRSS** | — | **357.4 MB** (365,972 kB) | 4,759.2 MB (4,873,436 kB) |
| **Semantic Top-1** | — | **78.3%** (18/23) | 26.1% (6/23) |
| **Semantic Top-3** | — | **87.0%** (20/23) | 26.1% (6/23) |
| **Semantic Top-5** | — | **91.3%** (21/23) | 26.1% (6/23) |
| **Semantic Top-8** | — | **91.3%** (21/23) | 26.1% (6/23) |
| **Semantic MRR** | — | **0.837** | 0.261 |
| **Semantic p95 Latency** | — | **3,426 ms** | 5,353 ms |
| **Hybrid Top-1** | — | **56.5%** (13/23) | 47.8% (11/23) |
| **Hybrid Top-3** | — | **73.9%** (17/23) | 69.6% (16/23) |
| **Hybrid Top-5** | — | **82.6%** (19/23) | 78.3% (18/23) |
| **Hybrid Top-8** | — | **87.0%** (20/23) | 82.6% (19/23) |
| **Hybrid MRR** | — | **0.664** | 0.596 |
| **Hybrid p95 Latency** | — | **3,412 ms** | 5,424 ms |
| **Lexical Top-1** | 21.7% (5/23) | 21.7% (5/23) | 21.7% (5/23) |
| **Lexical Top-5** | 26.1% (6/23) | 26.1% (6/23) | 26.1% (6/23) |
| **Lexical MRR** | 0.239 | 0.239 | 0.239 |
| **Lexical p95 Latency** | 3,329 ms | 3,329 ms | 3,963 ms |
| **Outcomes (Lex / Sem / Tie)**| — | 1 / 16 / 6 | 2 / 15 / 6 |

---

## 3. Case-by-Case Breakdown

| Case ID | Repo | Category | Lexical | `bge-small` Sem | `bge-small` Hyb | `qwen3` Sem | `qwen3` Hyb |
|---|---|---|---:|---:|---:|---:|---:|
| `express-lazy-router` | express | concept | 1 | 1 | 1 | 1 | 2 |
| `express-mount-middleware` | express | concept | - | 1 | 1 | 1 | 1 |
| `flask-view-dispatch` | flask | implementation | - | 1 | 1 | - | 5 |
| `flask-blueprint-registration` | flask | concept | - | 1 | 1 | - | 2 |
| `flask-request-context-session` | flask | concept | - | 2 | 5 | - | 3 |
| `gson-strictness` | gson | concept | 1 | 1 | 3 | - | 5 |
| `gson-reflective-fields` | gson | implementation | - | 1 | 6 | - | - |
| `gson-type-adapter` | gson | concept | - | - | - | - | - |
| `moshi-polymorphic-labels` | moshi | concept | 2 | 1 | 1 | - | 1 |
| `moshi-record-adapter` | moshi | implementation | - | 1 | 3 | - | - |
| `moshi-json-adapter-null-wrapper` | moshi | concept | - | 1 | 4 | - | 3 |
| `alamofire-interceptor-selection` | alamofire | concept | - | - | - | - | 7 |
| `alamofire-multipart-boundary` | alamofire | implementation | - | 1 | 1 | - | 1 |
| `alamofire-auth-refresh-window` | alamofire | concept | 1 | 1 | 1 | - | 1 |
| `cobra-command-execute` | cobra | implementation | - | 1 | 1 | 1 | 1 |
| `cobra-flag-completion` | cobra | implementation | 1 | 1 | 1 | 1 | 1 |
| `sinatra-route-dispatch` | sinatra | concept | - | 1 | 1 | 1 | 1 |
| `sinatra-route-compile` | sinatra | implementation | - | 1 | 1 | 1 | 1 |
| `nlohmann-json-pointer` | nlohmann-json | concept | - | 1 | 1 | - | 1 |
| `nlohmann-binary-reader` | nlohmann-json | implementation | - | 1 | 1 | - | 1 |
| `nlohmann-sax-parser` | nlohmann-json | concept | 1 | 4 | - | - | - |
| `newtonsoft-serializer-internal-reader` | newtonsoft-json | implementation | - | 1 | 2 | - | 1 |
| `jq-compile-bytecode` | jq | implementation | - | 2 | 2 | - | 2 |

---

## 4. Evidence Artifacts

The underlying JSON and Markdown evaluation results are stored under `docs/eval/semantic-value/results/`:
- **`bge-small-en-v1.5-f32`**:
  - `docs/eval/semantic-value/results/2026-09-10T21-01-40Z.json`
  - `docs/eval/semantic-value/results/2026-09-10T21-01-40Z.md`
- **`qwen3-0.6b-f16`**:
  - `docs/eval/semantic-value/results/2026-09-10T21-18-30Z.json`
  - `docs/eval/semantic-value/results/2026-09-10T21-18-30Z.md`

---

## 5. CodeRankEmbed Status

`CodeRankEmbed` was not evaluated because it is absent from the sidecar manifest.

- **Repository:** `anortham/julie-semantic-sidecar`
- **Manifest File:** `src/manifest.rs`
- **Sidecar Commit:** `9ed082ba511aa8b10c9e7b47110c3a4dd1e98d59`
- **Evidence:** Lines 84–121 in `manifest.rs` define `const PINS: &[ModelPin]` containing exclusively two entries:
  - Lines 85–102: `qwen3-0.6b-f16`
  - Lines 103–120: `bge-small-en-v1.5-f32`
- **Status:** Verified exclusion. Evaluating `CodeRankEmbed` requires an upstream sidecar release that converts the model to GGUF, validates the prompt formatting and MRL dimensionality, and registers a pin in `src/manifest.rs`.

---

## 6. Corpus Configuration

- **Scorecard Path:** `docs/eval/semantic-value/scorecard.toml`
- **Repositories (10 public repos):**
  - `/home/murphy/source/express` (JavaScript, 210 files)
  - `/home/murphy/source/flask` (Python, 229 files)
  - `/home/murphy/source/gson` (Java, 304 files)
  - `/home/murphy/source/moshi` (Java/Kotlin, 178 files)
  - `/home/murphy/source/Alamofire` (Swift, 567 files)
  - `/home/murphy/source/cobra` (Go, 64 files)
  - `/home/murphy/source/sinatra` (Ruby, 291 files)
  - `/home/murphy/source/nlohmann-json` (C++, 1,212 files)
  - `/home/murphy/source/Newtonsoft.Json` (C#, 1,171 files)
  - `/home/murphy/source/jq` (C, 421 files)
- **Target Verification:** All 23 target paths exist on the local filesystem.
- **Corpus Cleanliness:** `julie` repository cases were removed from the scorecard to prevent query token self-matching against the test configuration itself.
- **Baseline Comparison Note:** In accordance with instructions, this evaluation is self-contained across the refreshed 10-repo corpus and does not compare against the historical May Python baseline.

---

## 7. Recommendation

**Verdict:** Retain `bge-small-en-v1.5-f32` as the default model (`DEFAULT_NATIVE_MODEL = "bge-small-en-v1.5-f32"`). No code changes to `DEFAULT_NATIVE_MODEL` are required.
