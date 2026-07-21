# Partial-read patch pitfall

When a Julie file was only read with offset/limit pagination, `patch` may refuse broad replacements and report that the file should be re-read in full before overwriting.

Observed failure mode:
- `was last read with offset/limit pagination (partial view)`
- broad `old_string` matched many places (`Found 37 matches`)

Rules of thumb:
1. Re-read the whole file before any overwrite if the tool says the prior view was partial.
2. Use tighter, more unique context for `old_string` instead of a bare delimiter or closing brace.
3. Prefer the smallest safe patch over replacing a whole trailing block from a partial view.
4. If you are appending helpers or imports near file end, verify the current tail first so you do not collide with repeated braces or repeated helper stubs.
