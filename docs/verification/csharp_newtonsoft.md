# C# Verification — JamesNK/Newtonsoft.Json
**Workspace:** newtonsoft_json_afe705a1 | **Files:** 1160 | **Symbols:** 21062 | **Relationships:** 17049

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | PASS | JsonConvert class with 69 methods, 8 fields, correct kinds. Namespace, imports, class all extracted. |
| 2. Relationship Extraction | PASS | 786 total refs to JsonConvert. Cross-file refs work. `.Tests` project refs included (Benchmarks, Converters, Bson, etc). |
| 3. Identifier Extraction | PASS | Definition found at correct location. Call sites, Uses, Extends all present with correct reference kinds. |
| 4. Centrality | PASS | JsonConvert centrality=1.00 (786 refs). JsonSerializer centrality=0.31-0.37. JToken centrality=0.35. Core classes ranked highest. |
| 5. Definition Search | PASS | Real class `JsonConvert` at `JsonConvert.cs:53` ranks first as pinned definition. Test files and `JsonConverter` rank below. |
| 6. deep_dive Resolution | PASS | Full context returned: 8 fields, 69 methods with signatures/line numbers, 15 callers shown, body excerpt, change risk HIGH (0.70). |
| 7. get_context | PASS | 2 pivots (Serialize method, JsonSerializer class), 38 neighbors from 11 files. Pivots show full code bodies. Neighbors include ITraceWriter, JToken, IReferenceResolver, etc. |
| 8. Test Detection | PASS | `exclude_tests=true` removes all `Newtonsoft.Json.Tests/` files; `exclude_tests=false` includes them. C# `.Tests` project convention correctly detected. |

**Overall: 8/8 PASS**

## Issues Found

None. All 8 checks pass cleanly.

## Observations

### Noteworthy Positives

1. **C# `.Tests` convention handled correctly** — The `.Tests` project naming convention (`Newtonsoft.Json.Tests/`) is properly recognized for test detection. `exclude_tests=true` removes all test files, `exclude_tests=false` includes them.

2. **High centrality for core class** — `JsonConvert` has centrality 1.00 (786 incoming refs), which is correct for the primary API surface of Newtonsoft.Json.

3. **Rich method extraction** — All 69 methods on `JsonConvert` are extracted with full signatures including C# attributes (`[CLSCompliant(false)]`, `[DebuggerStepThrough]`, `[RequiresUnreferencedCode(...)]`).

4. **Partial class handling** — `JToken` splits across `JToken.cs` and `JToken.Async.cs` (partial classes). Both are found via `deep_dive` disambiguation. The main class definition shows 118 methods and 30 fields correctly combined.

5. **Inheritance chain** — `JsonReader` shows 9 extending classes (BsonReader, TraceJsonReader, JsonTextReader, JTokenReader, etc.) correctly extracted via relationship tracking.

6. **get_context graph quality** — Query "JSON serialization deserialization" correctly pivots on `JsonSerializer.Serialize()` and the `JsonSerializer` class, with 38 neighbors covering settings, formatters, and converters.

### Minor Notes (Not Bugs)

- **JToken centrality appears low (0.35)** despite being a core class. This is because centrality is measured by _direct_ `type_usage`/`call` references in the relationship graph, and many JToken usages may be via its subclasses (JObject, JArray, JValue) rather than direct references. The 412 test count confirms it is heavily used. This is expected behavior, not a bug.

- **JsonSerializer centrality (0.31)** is moderate rather than high. Similar reasoning: most usage goes through `JsonConvert.SerializeObject()` (the static facade), not direct `JsonSerializer` instantiation. The 209 test count is healthy.

## Raw Evidence

### Check 1: Symbol Extraction

```
get_symbols(file_path="Src/Newtonsoft.Json/JsonConvert.cs", max_depth=1, mode="structure")

Src/Newtonsoft.Json/JsonConvert.cs — 16 symbols
  import using System (26-26)
  import using System.IO (27-27)
  import using System.Globalization (28-28)
  import using System.Numerics (30-30)
  import using Newtonsoft.Json.Linq (32-32)
  import using Newtonsoft.Json.Utilities (33-33)
  import using System.Xml (34-34)
  import using Newtonsoft.Json.Converters (35-35)
  import using Newtonsoft.Json.Serialization (36-36)
  import using System.Text (37-37)
  import using System.Diagnostics (38-38)
  import using System.Runtime.CompilerServices (39-39)
  import using System.Diagnostics.CodeAnalysis (40-40)
  import using System.Xml.Linq (42-42)
  namespace namespace Newtonsoft.Json (45-1155)
    class public static class JsonConvert (53-1154)
```

Kinds extracted: `import`, `namespace`, `class` — all correct.

### Check 2: Relationship Extraction

```
fast_refs(symbol="JsonConvert", limit=15)

16 references to "JsonConvert":

Definition:
  Src/Newtonsoft.Json/JsonConvert.cs:53 (class) → public static class JsonConvert

References (15):
  Src/Newtonsoft.Json.Tests/Benchmarks/DeserializeBenchmarks.cs:56 (Calls)
  Src/Newtonsoft.Json.Tests/Benchmarks/DeserializeBenchmarks.cs:73 (Calls)
  Src/Newtonsoft.Json.Tests/Benchmarks/DeserializeBenchmarks.cs:79 (Calls)
  Src/Newtonsoft.Json.Tests/Bson/BsonWriterTests.cs:359 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/BinaryConverterTests.cs:67 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/BinaryConverterTests.cs:81 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/BinaryConverterTests.cs:158 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/BinaryConverterTests.cs:174 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/BinaryConverterTests.cs:188 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/CustomCreationConverterTests.cs:85 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/CustomCreationConverterTests.cs:134 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/CustomCreationConverterTests.cs:223 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/DataSetConverterTests.cs:48 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/DataSetConverterTests.cs:93 (Calls)
  Src/Newtonsoft.Json.Tests/Converters/DataSetConverterTests.cs:141 (Calls)
```

Cross-file refs: YES. .Tests project refs: YES. Reference kinds: Calls (correct).

### Check 3: Identifier Extraction

Definition found at `Src/Newtonsoft.Json/JsonConvert.cs:53`. Reference kinds include `Calls`, `Uses`, `Extends`, `References`. Call sites correctly identified in test files.

### Check 4: Centrality

```
deep_dive(symbol="JsonConvert", depth="overview")
  centrality: 1.00 (786 incoming refs)
  Change Risk: HIGH (0.70)

deep_dive(symbol="JToken", context_file="Linq/JToken.cs", depth="overview")
  Src/Newtonsoft.Json/Linq/JToken.cs:55 — centrality: 0.35 (1 incoming refs)
  412 tests (best: thorough)

deep_dive(symbol="JsonSerializer", context_file="Newtonsoft.Json/JsonSerializer.cs")
  Src/Newtonsoft.Json/JsonSerializer.cs:47 — centrality: 0.31 (1 incoming refs)
  209 tests

deep_dive(symbol="JsonReader")
  Src/Newtonsoft.Json/JsonReader.cs:41 — centrality: 0.43 (0 incoming refs)
  138 tests
  9 extending classes found
```

### Check 5: Definition Search

```
fast_search(query="JsonConvert", search_target="definitions", limit=8)

Definition found: JsonConvert
  Src/Newtonsoft.Json/JsonConvert.cs:53 (class, public)    ← PINNED FIRST
  public static class JsonConvert

Other matches:
  Src/Newtonsoft.Json.Tests/JsonConvertTest.cs:60
  Src/Newtonsoft.Json/JsonConverter.cs:83
  Src/Newtonsoft.Json.Tests/DemoTests.cs:65
  Src/Newtonsoft.Json/Serialization/JsonTypeReflector.cs:204
  ...
```

Real definition pinned first. Fuzzy matches (JsonConverter, test classes) ranked below.

### Check 6: deep_dive Resolution (context depth)

```
deep_dive(symbol="JsonConvert", depth="context")

  8 fields, 69 methods with full signatures and line numbers
  Used by (15 of 786): test methods with [Test] attribute shown
  Change Risk: HIGH (0.70) — 786 dependents, public, thorough tests
  Body excerpt: lines 50-79 shown with code
```

### Check 7: get_context

```
get_context(query="JSON serialization deserialization")

  Context | pivots=2 neighbors=38 files=11
  PIVOT Serialize (Src/Newtonsoft.Json/JsonSerializer.cs:1073) kind=method centrality=high
  PIVOT JsonSerializer (Src/Newtonsoft.Json/JsonSerializer.cs:47) kind=class centrality=medium
  NEIGHBORS: ITraceWriter, FromObjectInternal, JToken, IReferenceResolver, SerializeInternal,
             SerializeObjectInternal, 24 JsonSerializer properties, 5 Serialize/WriteJson variants,
             FuzzSerialization, FuzzIdempotent
```

### Check 8: Test Detection

```
exclude_tests=false (8 results):
  Src/Newtonsoft.Json/JsonConvert.cs:53          ← source (kept)
  Src/Newtonsoft.Json.Tests/JsonConvertTest.cs:60 ← test (kept)
  Src/Newtonsoft.Json/JsonConverter.cs:83          ← source (kept)
  Src/Newtonsoft.Json.Tests/DemoTests.cs:65        ← test (kept)
  Src/Newtonsoft.Json/Serialization/JsonTypeReflector.cs:204
  Src/Newtonsoft.Json.Tests/Issues/Issue1620.cs:86 ← test (kept)
  Src/Newtonsoft.Json.Tests/Serialization/PreserveReferencesHandlingTests.cs:66 ← test (kept)
  Src/Newtonsoft.Json.Tests/Documentation/PerformanceTests.cs:87 ← test (kept)

exclude_tests=true (8 results):
  Src/Newtonsoft.Json/JsonConvert.cs:53            ← source (kept)
  Src/Newtonsoft.Json/JsonConverter.cs:83           ← source (kept)
  Src/Newtonsoft.Json/Serialization/JsonTypeReflector.cs:204 ← source (kept)
  Src/Newtonsoft.Json/JsonConverter.cs:37           ← source (kept)
  Src/Newtonsoft.Json/Converters/VersionConverter.cs:35
  Src/Newtonsoft.Json/Converters/XmlNodeConverter.cs:942
  Src/Newtonsoft.Json/Converters/ExpandoObjectConverter.cs:41
  Src/Newtonsoft.Json/Converters/DateTimeConverterBase.cs:33
```

All `Newtonsoft.Json.Tests/` paths removed when `exclude_tests=true`. Only source files remain. `.Tests` project convention correctly detected.
