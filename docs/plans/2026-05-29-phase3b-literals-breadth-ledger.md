# Phase 3b — String-literal capture breadth ledger

Tracks the carrier-gated string-literal capture (Miller bridge Phase 3) across
all 34 languages to a 100% **implemented OR verified-N/A** state. Driven exactly
like the Phase 2 type-argument ledger. No "we'll get to it" bucket.

A language is **applicable** if it has idiomatic HTTP-client and/or DB-client
libraries whose calls take URL/SQL **string-literal arguments**. The capture arm
is config-free (emits a `Literal` for every string call-arg, `kind=Other`); the
`src/` carrier gate (`classify_literals_by_carrier`) sets `kind` and drops
non-carrier literals. A language contributes to Miller only when BOTH its
capture arm AND its `[literal_carriers]` TOML exist. **A capture arm with no
TOML carriers persists zero literals** (the gate drops them), so arms are safe
to land incrementally.

## The per-language template (3 worked reference legs)

Each arm lives in `crates/julie-extractors/src/<lang>/identifiers.rs`, hooked
into the existing call-node match arm as a **parallel emit** (it shares no code
with identifier extraction). Pattern:

1. From the call node, get the callee (`function`/callee field) and the
   arguments list.
2. Derive `carrier`: bare name for a plain identifier callee; the
   `receiver.method` (object.property) join for a member callee — so dotted
   config (`requests.get`) matches exactly and bare config (`execute`) matches
   any receiver via the gate's **last-segment rule** (`pool.query` → `query`).
3. For each argument (descending any keyword/wrapper node to its value), call
   `base.decode_string_literal(&value)`; on `Some`, `base.record_literal(&value,
   text, carrier.clone(), pos as u32, containing_symbol_id.clone())`.
4. `arg_position` is counted over the FULL argument list.

Shared, language-agnostic helpers in `base/extractor.rs`: `record_literal`,
`decode_string_literal` (delimiter strip + interpolation/substitution holes →
`{}` via node-kind substrings; `strip_string_delimiters` fallback). Reuse them —
do NOT reimplement decoding per language.

| Ref leg | Grammar family | Call node | Callee field | Carrier strategy | Arg wrapper |
|---------|----------------|-----------|--------------|------------------|-------------|
| TypeScript | call_expression | `call_expression` | `function` (identifier \| member_expression) | bare name / `object.property` | none (named args direct) |
| C# | invocation | `invocation_expression` | `function` (identifier \| generic_name \| member_access) | method name (generics stripped) | `argument` → last named child |
| Python | call | `call` | `function` (identifier \| attribute) | bare name / `object.attribute` | `keyword_argument` → `value` |

Tests mirror `crates/julie-extractors/src/tests/{typescript,csharp,python}/literals.rs`:
assert `literal_text` (incl. interpolation decode), `carrier`, `arg_position`,
`kind == Other`, and `containing_symbol_id.is_some()`. The extractor is
carrier-AGNOSTIC — it captures every string call-arg; the gate drops non-carriers
later (so do NOT assert dropping in the extractor test).

## Applicability matrix

Status: ✅ implemented (arm + TOML + test) · ⬜ pending · 🚫 verified-N/A.

| # | Language | Call node (`identifiers.rs`) | Status | Notes |
|---|----------|------------------------------|:------:|-------|
| 1 | TypeScript | `call_expression` | ✅ | reference leg |
| 2 | C# | `invocation_expression` | ✅ | reference leg |
| 3 | Python | `call` | ✅ | reference leg (`call` family) |
| 4 | JavaScript | `call_expression` | ✅ | mirrors TS leg; fetch/axios/ky/got/ofetch + bare DB verbs + Prisma. `b05b2da6` |
| 5 | Vue | `call_expression` | ✅ | mirrors TS leg (shared carriers). `b05b2da6` |
| 6 | VB.NET | `invocation_expression` | ✅ | mirrors C#; HttpClient async + Dapper/ADO.NET/EF verbs; interpolation→{}. `330f66d6` |
| 7 | Razor | `invocation_expression` | ✅ | mirrors C# (shared .NET carriers). `330f66d6` |
| 8 | Java | `method_invocation` | ⬜ | `object`/`name`/`arguments`; HttpClient/RestTemplate/JDBC |
| 9 | Kotlin | `call_expression` | ⬜ | OkHttp/Ktor/Exposed/JDBC |
| 10 | Scala | `call_expression` | ⬜ | sttp/requests-scala; Doobie/Slick (interp — capture static) |
| 11 | Go | `call_expression` | ✅ | `go_carrier` operand.field/bare; net/http dotted + database/sql+sqlx bare. `8781fe38` |
| 12 | Rust | `call_expression` (+ macro) | ⬜ | reqwest; sqlx `query!`/`query` — note macro args |
| 13 | Swift | `call_expression` | ✅ | URL(string:)+AF.request dotted; SQLite.swift/GRDB prepare/run/execute/scalar bare. `b05b2da6` |
| 14 | Dart | `call_expression` | ⬜ | http/Dio; sqflite `rawQuery`/`execute` |
| 15 | PHP | `function_call_expression`, `member_call_expression` | ⬜ | Guzzle/`Http::get`; PDO/mysqli `query`/`exec`/`prepare` |
| 16 | Ruby | `call` | ✅ | receiver.method/bare; Net::HTTP/HTTParty/RestClient/Faraday dotted + AR/mysql2/pg bare. `8781fe38` |
| 17 | Elixir | `call` | ✅ | Module.function/bare; HTTPoison/Req/Tesla dotted, Ecto/Postgrex bare. `b05b2da6` |
| 18 | R | `call` | ✅ | httr.GET/HEAD dotted-only (avoid base-R get/head); POST/PUT/etc bare; DBI db* verbs. Known limit: bare GET(url) dropped. `b05b2da6` |
| 19 | GDScript | `call`, `attribute_call` | ✅ | HTTPRequest request/request_raw; godot-sqlite query/query_with_bindings. `<next>` |
| 20 | Lua | `function_call` | ✅ | luasocket/luasec/lua-requests dotted; LuaSQL/lsqlite3 execute/exec/prepare (`:` method calls). `<next>` |
| 21 | QML | `call_expression` | ⬜ | XMLHttpRequest `open`/`send`; JS-in-QML |
| 22 | C | `call_expression` | ⬜ | libcurl `curl_easy_setopt`(CURLOPT_URL); `sqlite3_exec`/`PQexec`/`mysql_query` |
| 23 | C++ | `call_expression` | ⬜ | cpr/libcurl; sqlite3/`PQexec` |
| 24 | Zig | `call_expression` | ⬜ | std.http; verify HTTP/DB client carriers exist (may be thin) |
| 25 | Bash | command (`commands.rs`, `command`/`command_name`) | ⬜ | curl/wget URL args; psql/mysql `-c "SQL"`. Command grammar, not call_expression — different arm shape |
| 26 | PowerShell | `command`, `invocation_expression` | ⬜ | Invoke-RestMethod/Invoke-WebRequest; Invoke-Sqlcmd `-Query` |

### Verified-N/A (no HTTP/DB-client carrier concept) — with grammar evidence

| Language | Grammar evidence | Why N/A |
|----------|------------------|---------|
| SQL | has `invocation` (function/proc calls, `IdentifierKind::Call` at `sql/identifiers.rs:51`) | calls exist, but no HTTP client and SQL strings are not *passed to* a query call here — SQL **is** the query. No carrier concept; carrier config would be empty → zero gated literals. |
| CSS | has `call_expression` (`url()`/`calc()`/`rgb()`, `css/identifiers.rs:63`) | call shape exists, but `url()` is a CSS **asset** function, not an HTTP API or DB call. No HTTP/DB-client carrier; zero gated literals. |
| HTML | `calls` only inside embedded `<script>` (handled as JS) | HTML-proper has no call expressions; embedded JS is the JS extractor's domain. |
| Regex | no call node (`IdentifierKind::Call` used for backrefs at `regex/identifiers.rs:69`) | a pattern language; no function calls with string args. |
| JSON | no call node (only `mod.rs`, `relationships.rs`) | data format; no call expressions. |
| TOML | no call node | data format; no call expressions. |
| YAML | no call node | data format; no call expressions. |
| Markdown | no call node | prose/markup; no call expressions. |

## Carrier curation policy

- Bare entries match any receiver (last-segment) — use for distinctive DB verbs
  (`execute`, `query`, `prepare`) and unambiguous HTTP fns (`urlopen`).
- Dotted entries match exactly — use when a bare method would over-match
  (`requests.get` not bare `get`; `axios.get` not bare `get`).
- Generosity is deliberate (an unknown client is a one-line TOML add), but DO
  NOT add bare entries that flood (`get`/`post`/`set`/`run` without a receiver).
- `kind` is a read-time-reclassifiable hint; `literal_text` disambiguates, so
  modest over-capture is acceptable.

## Completion gate

Phase 3b is done when every row above is ✅ or 🚫, the polyglot extract-scan
integration test covers ≥1 language per grammar family end-to-end, and the FINAL
commit bumps `EXTRACT_CONTRACT_VERSION` 1→2 in lockstep with schema 28 and
records the final version triple in
`docs/plans/2026-05-29-extraction-enrichments-for-miller-bridge.md`.
