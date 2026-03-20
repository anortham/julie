# Julie Embedding Quality Benchmark

**Model:** `nomic-ai/CodeRankEmbed` (768 dimensions)
**Workspaces:** 19
**Pivots per workspace:** 5
**KNN limit:** 10
**Min similarity threshold:** 0.3

## 1. Coverage Summary

| Workspace | Language | Total | Embedded | Coverage | Embeddable % | Vars | Gap Kinds | Enriched |
|-----------|----------|-------|----------|----------|-------------|------|-----------|----------|
| jq_13566b9e | c | 12361 | 1882 | 15.2% | 30.6% | 1742 | 1195 | 271 |
| nlohmann-json_a5f86cd4 | cpp | 15411 | 1581 | 10.3% | 26.2% | 4325 | 3332 | 361 |
| newtonsoft_json_afe705a1 | csharp | 21062 | 3535 | 16.8% | 48.7% | 0 | 934 | 3999 |
| riverpod_a7fdc041 | dart | 28289 | 4870 | 17.2% | 66.4% | 0 | 525 | 1750 |
| phoenix_ac16deb4 | elixir | 15907 | 3366 | 21.2% | 87.0% | 0 | 0 | 47 |
| cobra_8b201fd3 | go | 1441 | 366 | 25.4% | 54.8% | 114 | 57 | 174 |
| guava_7e9af99a | java | 125984 | 37154 | 29.5% | 53.1% | 0 | 7814 | 12797 |
| express_8cefd559 | javascript | 5384 | 306 | 5.7% | 49.6% | 1495 | 0 | 695 |
| moshi_c9c5a600 | kotlin | 6603 | 1007 | 15.3% | 29.7% | 0 | 67 | 2279 |
| lite_f7e95a20 | lua | 27858 | 7827 | 28.1% | 20.2% | 742 | 0 | 268 |
| slim_dce0015d | php | 4031 | 516 | 12.8% | 25.2% | 2025 | 50 | 222 |
| flask_9045020a | python | 4291 | 528 | 12.3% | 41.8% | 1387 | 40 | 144 |
| sinatra_86eed2fe | ruby | 4290 | 946 | 22.1% | 32.0% | 2155 | 184 | 25 |
| julie_316c0b08 | rust | 53996 | 7824 | 14.5% | 82.4% | 0 | 193 | 969 |
| cats_c701f713 | scala | 22336 | 13769 | 61.6% | 69.0% | 149 | 4009 | 0 |
| alamofire_3d4cceb5 | swift | 20555 | 2358 | 11.5% | 31.2% | 0 | 134 | 5142 |
| labhandbookv2_67e8c1cf | typescript | 7306 | 1015 | 13.9% | 25.5% | 1511 | 51 | 383 |
| zod_df52de88 | typescript | 17055 | 2949 | 17.3% | 31.8% | 5533 | 1729 | 2329 |
| zls_4b29ec8b | zig | 10677 | 1588 | 14.9% | 20.9% | 1726 | 4394 | 1533 |

## 2. Quality Summary

| Workspace | Language | Pivots | Avg Top Sim | Avg Diversity | Avg NS Overlap | Cross-lang |
|-----------|----------|--------|-------------|---------------|----------------|------------|
| jq_13566b9e | c | 5/5 | 0.882 | 0.7 | 0.1 | no |
| nlohmann-json_a5f86cd4 | cpp | 5/5 | 0.831 | 0.54 | 0.32 | no |
| newtonsoft_json_afe705a1 | csharp | 5/5 | 0.875 | 0.72 | 0.2 | no |
| riverpod_a7fdc041 | dart | 5/5 | 0.815 | 0.66 | 0.32 | yes |
| phoenix_ac16deb4 | elixir | 5/5 | 0.692 | 0.78 | 0.32 | yes |
| cobra_8b201fd3 | go | 5/5 | 0.599 | 0.325 | 0.0 | no |
| guava_7e9af99a | java | 5/5 | 1.0 | 0.44 | 0.82 | no |
| express_8cefd559 | javascript | 5/5 | 0.884 | 0.46 | 0.28 | no |
| moshi_c9c5a600 | kotlin | 5/5 | 0.8 | 0.96 | 0.92 | yes |
| lite_f7e95a20 | lua | 5/5 | 1.0 | 0.72 | 0.3 | no |
| slim_dce0015d | php | 5/5 | 0.926 | 0.8 | 0.6 | no |
| flask_9045020a | python | 5/5 | 0.729 | 0.5 | 0.02 | no |
| sinatra_86eed2fe | ruby | 5/5 | 0.728 | 0.34 | 0.18 | no |
| julie_316c0b08 | rust | 5/5 | 0.732 | 0.96 | 0.78 | no |
| cats_c701f713 | scala | 5/5 | 0.944 | 0.54 | 0.44 | no |
| alamofire_3d4cceb5 | swift | 5/5 | 0.952 | 0.66 | 0.94 | no |
| labhandbookv2_67e8c1cf | typescript | 5/5 | 0.795 | 0.74 | 0.46 | yes |
| zod_df52de88 | typescript | 5/5 | 0.832 | 0.72 | 0.22 | no |
| zls_4b29ec8b | zig | 5/5 | 0.95 | 0.62 | 0.26 | no |

## 3. Aggregate Metrics

- **Total pivot queries:** 95
- **Avg top similarity:** 0.84
- **Avg mean similarity:** 0.662
- **Avg diversity (cross-file):** 0.641
- **Avg namespace overlap:** 0.394
- **Avg same-kind ratio:** 0.873
- **Cross-language results:** 12.6%

## 4. Detailed Results

### jq_13566b9e (c)

**Pivot:** `jv` (type, c, refs=1134)  
File: `src/jq.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.802 | `jv_kind` | type | `src/jv.h` |
| 2 | 0.780 | `jv_parser` | struct | `src/jv.h` |
| 3 | 0.694 | `jv` | union | `src/jv.h` |
| 4 | 0.679 | `inst` | struct | `src/compile.h` |
| 5 | 0.676 | `Bigint` | struct | `src/jv_dtoa.c` |
| 6 | 0.633 | `opcode` | struct | `src/bytecode.h` |
| 7 | 0.633 | `jv_refcnt` | struct | `src/jv.c` |
| 8 | 0.616 | `jv_refcnt` | struct | `src/jv.h` |
| 9 | 0.616 | `jv_refcnt` | struct | `src/jv.h` |
| 10 | 0.616 | `jv_parser` | struct | `src/jv.h` |

Quality: diversity=1.0, same_kind=0.1, ns_overlap=0.1, unique_files=5

**Pivot:** `jv` (union, c, refs=1003)  
File: `src/jv.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.694 | `jv` | type | `src/jq.h` |
| 2 | 0.690 | `U` | union | `src/jv_dtoa.c` |
| 3 | 0.588 | `jv_kind` | type | `src/jv.h` |
| 4 | 0.576 | `YYSTYPE` | union | `src/parser.c` |
| 5 | 0.576 | `YYSTYPE` | union | `src/parser.h` |
| 6 | 0.574 | `jv_parser` | struct | `src/jv.h` |
| 7 | 0.549 | `jvp_array` | struct | `src/jv.c` |
| 8 | 0.511 | `ULong` | type | `src/jv_dtoa.c` |
| 9 | 0.503 | `jv_refcnt` | struct | `src/jv.c` |
| 10 | 0.498 | `yytype_uint8` | type | `src/parser.c` |

Quality: diversity=0.8, same_kind=0.3, ns_overlap=0.1, unique_files=6

**Pivot:** `jv_free` (function, c, refs=605)  
File: `src/jv.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.975 | `jv_free` | function | `src/jv.c` |
| 2 | 0.801 | `jv_mem_free` | function | `src/jv_alloc.h` |
| 3 | 0.800 | `jvp_object_free` | function | `src/jv.c` |
| 4 | 0.788 | `jv_parser_free` | function | `src/jv.h` |
| 5 | 0.775 | `jv_mem_free` | function | `src/jv_alloc.c` |
| 6 | 0.771 | `jvp_number_free` | function | `src/jv.c` |
| 7 | 0.763 | `jvp_string_free` | function | `src/jv.c` |
| 8 | 0.741 | `jv_parser_free` | function | `src/jv_parse.c` |
| 9 | 0.733 | `jvp_array_free` | function | `src/jv.c` |
| 10 | 0.687 | `jv_false` | function | `src/jv.c` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.1, unique_files=5

**Pivot:** `jv_free` (function, c, refs=549)  
File: `src/jv.c`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.975 | `jv_free` | function | `src/jv.h` |
| 2 | 0.801 | `jvp_object_free` | function | `src/jv.c` |
| 3 | 0.789 | `jvp_number_free` | function | `src/jv.c` |
| 4 | 0.784 | `jv_mem_free` | function | `src/jv_alloc.h` |
| 5 | 0.769 | `jv_mem_free` | function | `src/jv_alloc.c` |
| 6 | 0.761 | `jv_parser_free` | function | `src/jv.h` |
| 7 | 0.750 | `jvp_string_free` | function | `src/jv.c` |
| 8 | 0.734 | `jv_parser_free` | function | `src/jv_parse.c` |
| 9 | 0.733 | `jvp_array_free` | function | `src/jv.c` |
| 10 | 0.670 | `jv_false` | function | `src/jv.c` |

Quality: diversity=0.5, same_kind=1.0, ns_overlap=0.1, unique_files=5

**Pivot:** `jv_copy` (function, c, refs=399)  
File: `src/jv.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.964 | `jv_copy` | function | `src/jv.c` |
| 2 | 0.648 | `jv_sort` | function | `src/jv.h` |
| 3 | 0.648 | `jv_set` | function | `src/jv.h` |
| 4 | 0.641 | `jv_equal` | function | `src/jv.h` |
| 5 | 0.626 | `jv_get` | function | `src/jv.h` |
| 6 | 0.622 | `jv_unique` | function | `src/jv.h` |
| 7 | 0.619 | `jv_object` | function | `src/jv.c` |
| 8 | 0.619 | `jv_object` | function | `src/jv.h` |
| 9 | 0.618 | `jv_object_merge` | function | `src/jv.h` |
| 10 | 0.604 | `jv_equal` | function | `src/jv.c` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.1, unique_files=2

### nlohmann-json_a5f86cd4 (cpp)

**Pivot:** `string` (method, cpp, refs=878)  
File: `include/nlohmann/detail/input/json_sax.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.925 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 2 | 0.864 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 3 | 0.765 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 4 | 0.737 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 5 | 0.687 | `string` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |
| 6 | 0.687 | `string` | method | `docs/mkdocs/docs/examples/sax_parse__binary.cpp` |
| 7 | 0.649 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 8 | 0.608 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 9 | 0.594 | `test` | method | `include/nlohmann/detail/meta/type_traits.hpp` |
| 10 | 0.593 | `test` | method | `include/nlohmann/detail/meta/type_traits.hpp` |

Quality: diversity=0.4, same_kind=1.0, ns_overlap=0.4, unique_files=4

**Pivot:** `string` (method, cpp, refs=878)  
File: `include/nlohmann/detail/input/json_sax.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.864 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 2 | 0.829 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 3 | 0.746 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 4 | 0.647 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 5 | 0.639 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 6 | 0.633 | `boolean` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 7 | 0.632 | `string` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |
| 8 | 0.632 | `string` | method | `docs/mkdocs/docs/examples/sax_parse__binary.cpp` |
| 9 | 0.581 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 10 | 0.556 | `get_cbor_string` | method | `include/nlohmann/detail/input/binary_reader.hpp` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.4, unique_files=4

**Pivot:** `string` (method, cpp, refs=878)  
File: `include/nlohmann/detail/input/json_sax.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.925 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 2 | 0.829 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 3 | 0.771 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 4 | 0.692 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 5 | 0.664 | `string` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |
| 6 | 0.664 | `string` | method | `docs/mkdocs/docs/examples/sax_parse__binary.cpp` |
| 7 | 0.647 | `boolean` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 8 | 0.625 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 9 | 0.608 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 10 | 0.559 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |

Quality: diversity=0.2, same_kind=1.0, ns_overlap=0.4, unique_files=3

**Pivot:** `size` (method, cpp, refs=804)  
File: `include/nlohmann/detail/meta/cpp_future.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.678 | `size` | function | `include/nlohmann/json.hpp` |
| 2 | 0.671 | `size` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 3 | 0.650 | `std::size_t` | function | `include/nlohmann/detail/string_concat.hpp` |
| 4 | 0.625 | `std::size_t` | function | `include/nlohmann/detail/string_concat.hpp` |
| 5 | 0.540 | `document_size` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 6 | 0.523 | `max_size` | function | `include/nlohmann/json.hpp` |
| 7 | 0.503 | `calc_bson_object_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |
| 8 | 0.498 | `size_and_type` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 9 | 0.497 | `concat_length` | function | `include/nlohmann/detail/string_concat.hpp` |
| 10 | 0.477 | `calc_bson_element_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |

Quality: diversity=1.0, same_kind=0.2, ns_overlap=0.2, unique_files=4

**Pivot:** `size` (function, cpp, refs=788)  
File: `include/nlohmann/json.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.762 | `max_size` | function | `include/nlohmann/json.hpp` |
| 2 | 0.678 | `size` | method | `include/nlohmann/detail/meta/cpp_future.hpp` |
| 3 | 0.639 | `count` | function | `include/nlohmann/ordered_map.hpp` |
| 4 | 0.580 | `std::size_t` | function | `include/nlohmann/detail/string_concat.hpp` |
| 5 | 0.564 | `size` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 6 | 0.554 | `count` | function | `include/nlohmann/json.hpp` |
| 7 | 0.542 | `std::size_t` | function | `include/nlohmann/detail/string_concat.hpp` |
| 8 | 0.526 | `size_and_type` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 9 | 0.500 | `calc_bson_element_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |
| 10 | 0.490 | `tuple_element` | class | `include/nlohmann/detail/iterators/iteration_proxy.hpp` |

Quality: diversity=0.8, same_kind=0.5, ns_overlap=0.2, unique_files=7

### newtonsoft_json_afe705a1 (csharp)

**Pivot:** `Value` (method, csharp, refs=1537)  
File: `Src/Newtonsoft.Json/Linq/Extensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.957 | `Value` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 2 | 0.651 | `Convert` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 3 | 0.628 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 4 | 0.584 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 5 | 0.556 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 6 | 0.554 | `explicit operator uint` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 7 | 0.546 | `AsJEnumerable` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 8 | 0.541 | `explicit operator ushort` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 9 | 0.538 | `explicit operator ulong` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 10 | 0.535 | `Convert` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.1, unique_files=2

**Pivot:** `Value` (method, csharp, refs=1537)  
File: `Src/Newtonsoft.Json/Linq/Extensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.957 | `Value` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 2 | 0.662 | `Convert` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 3 | 0.626 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 4 | 0.623 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 5 | 0.595 | `AsJEnumerable` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 6 | 0.565 | `explicit operator uint` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 7 | 0.564 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 8 | 0.552 | `Convert` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 9 | 0.552 | `explicit operator ulong` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 10 | 0.547 | `explicit operator double` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.1, unique_files=2

**Pivot:** `Value` (method, csharp, refs=1432)  
File: `Src/Newtonsoft.Json/Linq/JToken.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.606 | `Get` | method | `Src/Newtonsoft.Json/Utilities/ThreadSafeStore.cs` |
| 2 | 0.560 | `Convert` | method | `Src/Newtonsoft.Json/Serialization/FormatterConverter.cs` |
| 3 | 0.560 | `Convert` | method | `...tonsoft.Json/Serialization/JsonFormatterConverter.cs` |
| 4 | 0.487 | `Convert` | method | `Src/Newtonsoft.Json/Serialization/FormatterConverter.cs` |
| 5 | 0.487 | `Convert` | method | `...tonsoft.Json/Serialization/JsonFormatterConverter.cs` |
| 6 | 0.485 | `Value` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 7 | 0.470 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 8 | 0.465 | `TryGetValue` | method | `Src/Newtonsoft.Json/Utilities/DictionaryWrapper.cs` |
| 9 | 0.455 | `Value` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 10 | 0.454 | `AddValue` | method | `Src/Newtonsoft.Json/Utilities/ThreadSafeStore.cs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.2, unique_files=5

**Pivot:** `Read` (method, csharp, refs=1346)  
File: `Src/Newtonsoft.Json/Schema/JsonSchemaBuilder.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.856 | `Read` | method | `Src/Newtonsoft.Json/Schema/JsonSchema.cs` |
| 2 | 0.784 | `Read` | method | `Src/Newtonsoft.Json/Schema/JsonSchema.cs` |
| 3 | 0.559 | `ReadJson` | method | `Src/Newtonsoft.Json/JsonConverter.cs` |
| 4 | 0.555 | `ReadJson` | method | `Src/Newtonsoft.Json/Converters/BsonObjectIdConverter.cs` |
| 5 | 0.554 | `ReadJson` | method | `Src/Newtonsoft.Json/JsonConverter.cs` |
| 6 | 0.551 | `ReadJson` | method | `...ewtonsoft.Json/Converters/CustomCreationConverter.cs` |
| 7 | 0.551 | `ReadJson` | method | `Src/Newtonsoft.Json/Converters/DataSetConverter.cs` |
| 8 | 0.551 | `ReadJson` | method | `...nsoft.Json/Converters/JavaScriptDateTimeConverter.cs` |
| 9 | 0.551 | `ReadJson` | method | `Src/Newtonsoft.Json/Converters/XmlNodeConverter.cs` |
| 10 | 0.551 | `ReadJson` | method | `Src/Newtonsoft.Json/Converters/BinaryConverter.cs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.2, unique_files=8

**Pivot:** `Read` (method, csharp, refs=1346)  
File: `Src/Newtonsoft.Json/Linq/JTokenReader.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `Read` | method | `Src/Newtonsoft.Json/Bson/BsonReader.cs` |
| 2 | 1.000 | `Read` | method | `Src/Newtonsoft.Json/JsonTextReader.cs` |
| 3 | 1.000 | `Read` | method | `Src/Newtonsoft.Json/JsonValidatingReader.cs` |
| 4 | 0.923 | `Read` | method | `Src/Newtonsoft.Json/JsonReader.cs` |
| 5 | 0.815 | `ReadAsBoolean` | method | `Src/Newtonsoft.Json/JsonTextReader.cs` |
| 6 | 0.815 | `ReadAsBoolean` | method | `Src/Newtonsoft.Json/JsonValidatingReader.cs` |
| 7 | 0.753 | `ReadAsBoolean` | method | `Src/Newtonsoft.Json/JsonReader.cs` |
| 8 | 0.719 | `ReadAsString` | method | `Src/Newtonsoft.Json/JsonTextReader.cs` |
| 9 | 0.707 | `ReadAsBytes` | method | `Src/Newtonsoft.Json/JsonTextReader.cs` |
| 10 | 0.705 | `ReadAsDouble` | method | `Src/Newtonsoft.Json/JsonTextReader.cs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.4, unique_files=4

### riverpod_a7fdc041 (dart)

**Pivot:** `container` (method, dart, refs=2583)  
File: `packages/flutter_riverpod/lib/src/core/provider_scope.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.557 | `ProviderContainer` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 2 | 0.524 | `ProviderContainer` | method | `...es/flutter_riverpod/lib/src/core/provider_scope.dart` |
| 3 | 0.470 | `ProviderScope` | class | `...es/flutter_riverpod/lib/src/core/provider_scope.dart` |
| 4 | 0.466 | `findDeepestTransitiveDependencyProviderContainer` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 5 | 0.436 | `_getParent` | function | `...es/flutter_riverpod/lib/src/core/provider_scope.dart` |
| 6 | 0.418 | `handleProviderContainerInstanceCreation` | method | `...ts/scoped_providers_should_specify_dependencies.dart` |
| 7 | 0.404 | `message` | variable | `website/i18n/fr/code.json` |
| 8 | 0.400 | `ProviderOrFamily` | function | `packages/riverpod/lib/src/core/foundation.dart` |
| 9 | 0.397 | `_SearchHintContainer` | class | `examples/marvel/lib/src/widgets/search_bar.dart` |
| 10 | 0.395 | `call` | method | `packages/riverpod/lib/src/builder.dart` |

Quality: diversity=0.7, same_kind=0.3, ns_overlap=0.0, unique_files=7

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/riverpod/lib/src/core/provider_subscription.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.629 | `read` | function | `...ges/riverpod/lib/src/core/provider_subscription.dart` |
| 2 | 0.566 | `readSelf` | method | `packages/riverpod/lib/src/core/element.dart` |
| 3 | 0.541 | `read` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 4 | 0.523 | `read` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |
| 5 | 0.516 | `read` | function | `packages/riverpod/lib/src/core/ref.dart` |
| 6 | 0.508 | `read` | function | `packages/flutter_riverpod/lib/src/core/consumer.dart` |
| 7 | 0.432 | `_callRead` | function | `...ges/riverpod/lib/src/core/provider_subscription.dart` |
| 8 | 0.427 | `readProviderElement` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 9 | 0.419 | `refresh` | function | `packages/flutter_riverpod/lib/src/core/consumer.dart` |
| 10 | 0.418 | `read` | function | `packages/riverpod/lib/src/core/persist.dart` |

Quality: diversity=0.8, same_kind=0.9, ns_overlap=0.6, unique_files=7

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/riverpod/lib/src/core/persist.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.960 | `read` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 2 | 0.874 | `read` | function | `packages/riverpod_sqflite/lib/src/riverpod_sqflite.dart` |
| 3 | 0.609 | `delete` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 4 | 0.514 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 5 | 0.510 | `delete` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 6 | 0.469 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 7 | 0.422 | `delete` | function | `packages/riverpod_sqflite/lib/src/riverpod_sqflite.dart` |
| 8 | 0.422 | `read` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |
| 9 | 0.418 | `read` | function | `...ges/riverpod/lib/src/core/provider_subscription.dart` |
| 10 | 0.410 | `read` | function | `packages/riverpod/lib/src/core/ref.dart` |

Quality: diversity=0.5, same_kind=1.0, ns_overlap=0.5, unique_files=5

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/flutter_riverpod/lib/src/core/widget_ref.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.970 | `read` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 2 | 0.956 | `read` | function | `packages/riverpod/lib/src/core/ref.dart` |
| 3 | 0.921 | `read` | function | `packages/flutter_riverpod/lib/src/core/consumer.dart` |
| 4 | 0.685 | `readProviderElement` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 5 | 0.674 | `get` | function | `packages/riverpod/lib/src/core/mutations.dart` |
| 6 | 0.651 | `_readProviderElement` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 7 | 0.621 | `watch` | function | `packages/riverpod/lib/src/core/ref.dart` |
| 8 | 0.594 | `readSelf` | method | `packages/riverpod/lib/src/core/element.dart` |
| 9 | 0.566 | `watch` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |
| 10 | 0.561 | `listen` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |

Quality: diversity=0.8, same_kind=0.9, ns_overlap=0.3, unique_files=6

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/riverpod/lib/src/core/persist.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.960 | `read` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 2 | 0.902 | `read` | function | `packages/riverpod_sqflite/lib/src/riverpod_sqflite.dart` |
| 3 | 0.584 | `delete` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 4 | 0.561 | `delete` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 5 | 0.530 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 6 | 0.530 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 7 | 0.472 | `delete` | function | `packages/riverpod_sqflite/lib/src/riverpod_sqflite.dart` |
| 8 | 0.445 | `_callEncode` | method | `...iverpod/lib/src/core/provider/notifier_provider.dart` |
| 9 | 0.419 | `_callEncode` | method | `...iverpod/lib/src/core/provider/notifier_provider.dart` |
| 10 | 0.419 | `_callEncode` | method | `...iverpod/lib/src/core/provider/notifier_provider.dart` |

Quality: diversity=0.5, same_kind=0.7, ns_overlap=0.2, unique_files=3

### phoenix_ac16deb4 (elixir)

**Pivot:** `inspect` (function, elixir, refs=419)  
File: `lib/phoenix/socket/message.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.543 | `Inspect` | class | `lib/phoenix/socket/message.ex` |
| 2 | 0.534 | `t` | type | `lib/phoenix/socket/message.ex` |
| 3 | 0.518 | `Phoenix.Socket.Message` | struct | `lib/phoenix/socket/message.ex` |
| 4 | 0.476 | `decode!` | function | `lib/phoenix/socket/serializer.ex` |
| 5 | 0.464 | `list` | function | `lib/phoenix/presence.ex` |
| 6 | 0.456 | `join` | function | `lib/phoenix/channel/server.ex` |
| 7 | 0.452 | `socket_dispatch` | function | `lib/phoenix/endpoint.ex` |
| 8 | 0.452 | `Phoenix.Socket.Message` | module | `lib/phoenix/socket/message.ex` |
| 9 | 0.436 | `phoenix_socket_drain` | function | `lib/phoenix/logger.ex` |
| 10 | 0.432 | `Phoenix.Socket` | struct | `lib/phoenix/socket.ex` |

Quality: diversity=0.6, same_kind=0.5, ns_overlap=0.0, unique_files=7

**Pivot:** `join` (function, elixir, refs=414)  
File: `priv/templates/phx.gen.channel/channel.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.611 | `join` | function | `lib/phoenix/channel.ex` |
| 2 | 0.588 | `join` | function | `lib/phoenix/channel/server.ex` |
| 3 | 0.489 | `channel_join` | function | `lib/phoenix/channel/server.ex` |
| 4 | 0.440 | `handle_in` | function | `priv/templates/phx.gen.channel/channel.ex` |
| 5 | 0.433 | `handle_in` | function | `priv/templates/phx.gen.channel/channel.ex` |
| 6 | 0.424 | `__in__` | function | `lib/phoenix/socket.ex` |
| 7 | 0.415 | `init_join` | function | `lib/phoenix/channel/server.ex` |
| 8 | 0.413 | `handle_join` | function | `lib/phoenix/presence.ex` |
| 9 | 0.411 | `handle_in` | function | `lib/phoenix/socket.ex` |
| 10 | 0.409 | `socket_ref` | function | `lib/phoenix/channel.ex` |

Quality: diversity=0.8, same_kind=1.0, ns_overlap=0.2, unique_files=5

**Pivot:** `join` (function, elixir, refs=414)  
File: `lib/phoenix/channel.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.611 | `join` | function | `priv/templates/phx.gen.channel/channel.ex` |
| 2 | 0.588 | `channel_join` | function | `lib/phoenix/channel/server.ex` |
| 3 | 0.581 | `join` | function | `lib/phoenix/channel/server.ex` |
| 4 | 0.552 | `init_join` | function | `lib/phoenix/channel/server.ex` |
| 5 | 0.518 | `handle_join` | function | `lib/phoenix/presence.ex` |
| 6 | 0.480 | `reply` | function | `lib/phoenix/channel/server.ex` |
| 7 | 0.474 | `reply` | function | `lib/phoenix/channel.ex` |
| 8 | 0.463 | `broadcast!` | function | `lib/phoenix/endpoint.ex` |
| 9 | 0.463 | `join` | method | `assets/js/phoenix/channel.js` |
| 10 | 0.463 | `join` | method | `priv/static/phoenix.cjs.js` |

Quality: diversity=0.9, same_kind=0.8, ns_overlap=0.4, unique_files=7

**Pivot:** `join` (method, javascript, refs=414)  
File: `assets/js/phoenix/channel.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `join` | method | `priv/static/phoenix.cjs.js` |
| 2 | 1.000 | `join` | method | `priv/static/phoenix.js` |
| 3 | 1.000 | `join` | method | `priv/static/phoenix.mjs` |
| 4 | 0.634 | `leave` | method | `assets/js/phoenix/channel.js` |
| 5 | 0.634 | `leave` | method | `priv/static/phoenix.cjs.js` |
| 6 | 0.634 | `leave` | method | `priv/static/phoenix.js` |
| 7 | 0.634 | `leave` | method | `priv/static/phoenix.mjs` |
| 8 | 0.625 | `join` | function | `lib/phoenix/channel/server.ex` |
| 9 | 0.585 | `rejoin` | method | `assets/js/phoenix/channel.js` |
| 10 | 0.585 | `rejoin` | method | `priv/static/phoenix.cjs.js` |

Quality: diversity=0.8, same_kind=0.9, ns_overlap=0.4, unique_files=5

**Pivot:** `join` (function, elixir, refs=413)  
File: `lib/phoenix/channel/server.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.697 | `channel_join` | function | `lib/phoenix/channel/server.ex` |
| 2 | 0.625 | `join` | method | `assets/js/phoenix/channel.js` |
| 3 | 0.625 | `join` | method | `priv/static/phoenix.cjs.js` |
| 4 | 0.625 | `join` | method | `priv/static/phoenix.js` |
| 5 | 0.625 | `join` | method | `priv/static/phoenix.mjs` |
| 6 | 0.588 | `join` | function | `priv/templates/phx.gen.channel/channel.ex` |
| 7 | 0.581 | `join` | function | `lib/phoenix/channel.ex` |
| 8 | 0.579 | `init_join` | function | `lib/phoenix/channel/server.ex` |
| 9 | 0.521 | `broadcast` | function | `lib/phoenix/channel.ex` |
| 10 | 0.511 | `broadcast!` | function | `lib/phoenix/channel.ex` |

Quality: diversity=0.8, same_kind=0.6, ns_overlap=0.6, unique_files=7

### cobra_8b201fd3 (go)

**Pivot:** `Flags` (method, go, refs=170)  
File: `command.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.676 | `LocalFlags` | method | `command.go` |
| 2 | 0.630 | `Groups` | method | `command.go` |
| 3 | 0.613 | `PersistentFlags` | method | `command.go` |
| 4 | 0.606 | `DebugFlags` | method | `command.go` |
| 5 | 0.578 | `InheritedFlags` | method | `command.go` |
| 6 | 0.561 | `HasFlags` | method | `command.go` |
| 7 | 0.542 | `Context` | method | `command.go` |
| 8 | 0.538 | `ResetFlags` | method | `command.go` |
| 9 | 0.532 | `Commands` | method | `command.go` |
| 10 | 0.521 | `Flag` | method | `command.go` |

Quality: diversity=0.0, same_kind=1.0, ns_overlap=0.0, unique_files=1

**Pivot:** `AddCommand` (method, go, refs=156)  
File: `command.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.793 | `AddGroup` | method | `command.go` |
| 2 | 0.735 | `RemoveCommand` | method | `command.go` |
| 3 | 0.455 | `Parent` | method | `command.go` |
| 4 | 0.430 | `Commands` | method | `command.go` |
| 5 | 0.410 | `HasParent` | method | `command.go` |
| 6 | 0.407 | `SetArgs` | method | `command.go` |
| 7 | 0.392 | `SetContext` | method | `command.go` |
| 8 | 0.381 | `Print` | method | `command.go` |
| 9 | 0.380 | `CommandPath` | method | `command.go` |
| 10 | 0.368 | `Traverse` | method | `command.go` |

Quality: diversity=0.0, same_kind=1.0, ns_overlap=0.0, unique_files=1

**Pivot:** `Name` (method, go, refs=116)  
File: `command.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.631 | `DisplayName` | method | `command.go` |
| 2 | 0.583 | `Parent` | method | `command.go` |
| 3 | 0.541 | `Root` | method | `command.go` |
| 4 | 0.518 | `NamePadding` | method | `command.go` |
| 5 | 0.514 | `Help` | method | `command.go` |
| 6 | 0.500 | `Usage` | method | `command.go` |
| 7 | 0.495 | `CalledAs` | method | `command.go` |
| 8 | 0.472 | `UsageString` | method | `command.go` |
| 9 | 0.470 | `Context` | method | `command.go` |
| 10 | 0.465 | `Runnable` | method | `command.go` |

Quality: diversity=0.0, same_kind=1.0, ns_overlap=0.0, unique_files=1

**Pivot:** `Error` (method, go, refs=77)  
File: `completions.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.528 | `flagCompError` | class | `completions.go` |
| 2 | 0.447 | `FlagErrorFunc` | method | `command.go` |
| 3 | 0.410 | `CompError` | function | `completions.go` |
| 4 | 0.376 | `Help` | method | `command.go` |
| 5 | 0.351 | `SetFlagErrorFunc` | method | `command.go` |
| 6 | 0.330 | `ErrPrefix` | method | `command.go` |
| 7 | 0.324 | `Usage` | method | `command.go` |
| 8 | 0.307 | `Name` | method | `command.go` |

Quality: diversity=0.75, same_kind=0.75, ns_overlap=0.0, unique_files=2

**Pivot:** `WriteStringAndCheck` (function, go, refs=61)  
File: `cobra.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.365 | `writeRequiredFlag` | function | `bash_completions.go` |
| 2 | 0.345 | `writeFlag` | function | `bash_completions.go` |
| 3 | 0.341 | `writeCommands` | function | `bash_completions.go` |
| 4 | 0.334 | `writeFlags` | function | `bash_completions.go` |
| 5 | 0.333 | `stringInSlice` | function | `cobra.go` |
| 6 | 0.313 | `writeRequiredNouns` | function | `bash_completions.go` |
| 7 | 0.310 | `writePreamble` | function | `bash_completions.go` |
| 8 | 0.309 | `writeShortFlag` | function | `bash_completions.go` |

Quality: diversity=0.875, same_kind=1.0, ns_overlap=0.0, unique_files=2

### guava_7e9af99a (java)

**Pivot:** `of` (method, java, refs=6412)  
File: `guava-gwt/src-super/com/google/common/collect/super/com/google/common/collect/ImmutableSortedMap.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 2 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 3 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 4 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 5 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 6 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 7 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 8 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 9 | 0.909 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 10 | 0.859 | `of` | method | `...ct/super/com/google/common/collect/ImmutableMap.java` |

Quality: diversity=0.1, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `of` (method, java, refs=6412)  
File: `guava-gwt/src-super/com/google/common/collect/super/com/google/common/collect/ImmutableSortedMap.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 2 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 3 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 4 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 5 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 6 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 7 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 8 | 1.000 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 9 | 0.909 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 10 | 0.859 | `of` | method | `...ct/super/com/google/common/collect/ImmutableMap.java` |

Quality: diversity=0.1, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `of` (method, java, refs=6412)  
File: `guava/src/com/google/common/primitives/ImmutableIntArray.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 2 | 0.969 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 3 | 0.969 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 4 | 0.919 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 5 | 0.919 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 6 | 0.885 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 7 | 0.885 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 8 | 0.864 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 9 | 0.864 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 10 | 0.803 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=1.0, unique_files=3

**Pivot:** `of` (method, java, refs=6412)  
File: `guava/src/com/google/common/base/Optional.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `android/guava/src/com/google/common/base/Optional.java` |
| 2 | 0.729 | `fromNullable` | method | `android/guava/src/com/google/common/base/Optional.java` |
| 3 | 0.729 | `fromNullable` | method | `guava/src/com/google/common/base/Optional.java` |
| 4 | 0.627 | `absent` | method | `android/guava/src/com/google/common/base/Optional.java` |
| 5 | 0.627 | `absent` | method | `guava/src/com/google/common/base/Optional.java` |
| 6 | 0.604 | `withType` | method | `android/guava/src/com/google/common/base/Absent.java` |
| 7 | 0.604 | `withType` | method | `guava/src/com/google/common/base/Absent.java` |
| 8 | 0.573 | `or` | method | `android/guava/src/com/google/common/base/Present.java` |
| 9 | 0.573 | `or` | method | `guava/src/com/google/common/base/Present.java` |
| 10 | 0.558 | `orNull` | method | `android/guava/src/com/google/common/base/Present.java` |

Quality: diversity=0.8, same_kind=1.0, ns_overlap=0.1, unique_files=6

**Pivot:** `of` (method, java, refs=6412)  
File: `guava/src/com/google/common/primitives/ImmutableLongArray.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 2 | 0.964 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 3 | 0.964 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 4 | 0.853 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 5 | 0.853 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 6 | 0.836 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 7 | 0.836 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 8 | 0.826 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 9 | 0.826 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 10 | 0.812 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=1.0, unique_files=3

### express_8cefd559 (javascript)

**Pivot:** `get` (method, javascript, refs=1024)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.812 | `get` | function | `lib/application.js` |
| 2 | 0.781 | `get` | method | `examples/route-map/index.js` |
| 3 | 0.701 | `get` | function | `examples/route-map/index.js` |
| 4 | 0.526 | `get` | method | `lib/response.js` |
| 5 | 0.483 | `json` | method | `examples/content-negotiation/index.js` |
| 6 | 0.483 | `json` | method | `examples/error-pages/index.js` |
| 7 | 0.467 | `res.get` | function | `lib/response.js` |
| 8 | 0.466 | `show` | function | `examples/resource/index.js` |
| 9 | 0.461 | `html` | method | `examples/content-negotiation/index.js` |
| 10 | 0.461 | `html` | method | `examples/error-pages/index.js` |

Quality: diversity=0.9, same_kind=0.6, ns_overlap=0.5, unique_files=6

**Pivot:** `get` (function, javascript, refs=1024)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.812 | `get` | method | `lib/application.js` |
| 2 | 0.742 | `get` | function | `examples/route-map/index.js` |
| 3 | 0.635 | `get` | method | `examples/route-map/index.js` |
| 4 | 0.478 | `html` | function | `examples/content-negotiation/index.js` |
| 5 | 0.478 | `html` | function | `examples/error-pages/index.js` |
| 6 | 0.478 | `html` | function | `lib/response.js` |
| 7 | 0.477 | `json` | function | `examples/content-negotiation/index.js` |
| 8 | 0.477 | `json` | function | `examples/error-pages/index.js` |
| 9 | 0.477 | `default` | function | `examples/error-pages/index.js` |
| 10 | 0.477 | `default` | function | `lib/response.js` |

Quality: diversity=0.9, same_kind=0.8, ns_overlap=0.3, unique_files=5

**Pivot:** `get` (method, javascript, refs=1012)  
File: `lib/response.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.908 | `res.get` | function | `lib/response.js` |
| 2 | 0.626 | `header` | method | `lib/response.js` |
| 3 | 0.615 | `res.header` | function | `lib/response.js` |
| 4 | 0.594 | `append` | method | `lib/response.js` |
| 5 | 0.573 | `res.append` | function | `lib/response.js` |
| 6 | 0.527 | `get` | method | `examples/route-map/index.js` |
| 7 | 0.526 | `get` | method | `lib/application.js` |
| 8 | 0.507 | `get` | function | `examples/route-map/index.js` |
| 9 | 0.479 | `vary` | method | `lib/response.js` |
| 10 | 0.465 | `header` | method | `lib/request.js` |

Quality: diversity=0.4, same_kind=0.6, ns_overlap=0.4, unique_files=4

**Pivot:** `use` (method, javascript, refs=641)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.944 | `app.use` | function | `lib/application.js` |
| 2 | 0.718 | `route` | method | `lib/application.js` |
| 3 | 0.606 | `app.route` | function | `lib/application.js` |
| 4 | 0.581 | `handle` | method | `lib/application.js` |
| 5 | 0.579 | `map` | method | `examples/route-map/index.js` |
| 6 | 0.549 | `param` | method | `lib/application.js` |
| 7 | 0.499 | `path` | method | `lib/application.js` |
| 8 | 0.484 | `all` | method | `lib/application.js` |
| 9 | 0.483 | `app[method]` | function | `lib/application.js` |
| 10 | 0.483 | `enable` | method | `lib/application.js` |

Quality: diversity=0.1, same_kind=0.7, ns_overlap=0.1, unique_files=2

**Pivot:** `set` (method, javascript, refs=593)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.943 | `app.set` | function | `lib/application.js` |
| 2 | 0.606 | `enable` | method | `lib/application.js` |
| 3 | 0.584 | `app.enable` | function | `lib/application.js` |
| 4 | 0.584 | `enabled` | method | `lib/application.js` |
| 5 | 0.577 | `disable` | method | `lib/application.js` |
| 6 | 0.555 | `app.disable` | function | `lib/application.js` |
| 7 | 0.488 | `disabled` | method | `lib/application.js` |
| 8 | 0.485 | `app.enabled` | function | `lib/application.js` |
| 9 | 0.455 | `path` | method | `lib/application.js` |
| 10 | 0.429 | `app.disabled` | function | `lib/application.js` |

Quality: diversity=0.0, same_kind=0.5, ns_overlap=0.1, unique_files=1

### moshi_c9c5a600 (kotlin)

**Pivot:** `fromJson` (method, kotlin, refs=435)  
File: `moshi-adapters/src/main/java/com/squareup/moshi/adapters/EnumJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.735 | `EnumJsonAdapter` | class | `.../java/com/squareup/moshi/adapters/EnumJsonAdapter.kt` |
| 2 | 0.722 | `FallbackEnumJsonAdapter` | class | `...in/java/com/squareup/moshi/recipes/FallbackEnum.java` |
| 3 | 0.718 | `toJson` | method | `.../java/com/squareup/moshi/adapters/EnumJsonAdapter.kt` |
| 4 | 0.677 | `EnumJsonAdapter` | class | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 5 | 0.673 | `fromJson` | method | `...a/com/squareup/moshi/internal/NullSafeJsonAdapter.kt` |
| 6 | 0.672 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/Moshi.kt` |
| 7 | 0.665 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 8 | 0.664 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 9 | 0.664 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 10 | 0.664 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |

Quality: diversity=0.8, same_kind=0.7, ns_overlap=0.6, unique_files=6

**Pivot:** `fromJson` (method, kotlin, refs=435)  
File: `moshi/src/main/java/com/squareup/moshi/internal/RecordJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.793 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/Moshi.kt` |
| 2 | 0.762 | `fromJson` | method | `...va/com/squareup/moshi/internal/NonNullJsonAdapter.kt` |
| 3 | 0.757 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 4 | 0.757 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 5 | 0.757 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 6 | 0.757 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 7 | 0.751 | `fromJson` | method | `...a/com/squareup/moshi/internal/NullSafeJsonAdapter.kt` |
| 8 | 0.742 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 9 | 0.729 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 10 | 0.707 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=6

**Pivot:** `fromJson` (method, kotlin, refs=435)  
File: `moshi-adapters/src/main/java/com/squareup/moshi/adapters/Rfc3339DateJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.910 | `fromJson` | method | `...in/java/com/squareup/moshi/Rfc3339DateJsonAdapter.kt` |
| 2 | 0.741 | `fromJson` | method | `...a/com/squareup/moshi/internal/NullSafeJsonAdapter.kt` |
| 3 | 0.728 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 4 | 0.728 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 5 | 0.728 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 6 | 0.728 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 7 | 0.727 | `fromJson` | method | `...va/com/squareup/moshi/internal/NonNullJsonAdapter.kt` |
| 8 | 0.722 | `fromJson` | method | `...reup/moshi/recipes/DefaultOnDataMismatchAdapter.java` |
| 9 | 0.722 | `fromJson` | method | `...in/java/com/squareup/moshi/recipes/FallbackEnum.java` |
| 10 | 0.716 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=8

**Pivot:** `fromJson` (method, kotlin, refs=434)  
File: `moshi/src/main/java/com/squareup/moshi/internal/ArrayJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.782 | `fromJson` | method | `...com/squareup/moshi/internal/CollectionJsonAdapter.kt` |
| 2 | 0.762 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |
| 3 | 0.762 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |
| 4 | 0.762 | `fromJson` | method | `...com/squareup/moshi/internal/AdapterMethodsFactory.kt` |
| 5 | 0.735 | `fromJson` | method | `...va/com/squareup/moshi/internal/NonNullJsonAdapter.kt` |
| 6 | 0.734 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 7 | 0.734 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 8 | 0.734 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 9 | 0.734 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 10 | 0.731 | `fromJson` | method | `...a/com/squareup/moshi/internal/NullSafeJsonAdapter.kt` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=7

**Pivot:** `fromJson` (method, kotlin, refs=434)  
File: `moshi/src/main/java/com/squareup/moshi/internal/CollectionJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.782 | `fromJson` | method | `...java/com/squareup/moshi/internal/ArrayJsonAdapter.kt` |
| 2 | 0.726 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 3 | 0.726 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 4 | 0.726 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 5 | 0.726 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 6 | 0.715 | `fromJson` | method | `...va/com/squareup/moshi/internal/NonNullJsonAdapter.kt` |
| 7 | 0.713 | `fromJson` | method | `...a/com/squareup/moshi/internal/NullSafeJsonAdapter.kt` |
| 8 | 0.677 | `fromJson` | method | `...reup/moshi/recipes/DefaultOnDataMismatchAdapter.java` |
| 9 | 0.677 | `fromJson` | method | `...in/java/com/squareup/moshi/recipes/FallbackEnum.java` |
| 10 | 0.677 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=8

### lite_f7e95a20 (lua)

**Pivot:** `GLenum` (type, c, refs=1400)  
File: `winlib/SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLenum` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.904 | `GLenum` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.904 | `GLenum` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.764 | `GLubyte` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 5 | 0.764 | `GLubyte` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 6 | 0.724 | `GLint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.724 | `GLint` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.713 | `EGLenum` | type | `.../SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_egl.h` |
| 9 | 0.713 | `EGLenum` | type | `...DL2-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_egl.h` |
| 10 | 0.708 | `GLuint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.3, unique_files=6

**Pivot:** `GLenum` (type, c, refs=1400)  
File: `winlib/SDL2-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLenum` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.904 | `GLenum` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.904 | `GLenum` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.764 | `GLubyte` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 5 | 0.764 | `GLubyte` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 6 | 0.724 | `GLint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.724 | `GLint` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.713 | `EGLenum` | type | `.../SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_egl.h` |
| 9 | 0.713 | `EGLenum` | type | `...DL2-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_egl.h` |
| 10 | 0.708 | `GLuint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.8, same_kind=1.0, ns_overlap=0.3, unique_files=6

**Pivot:** `GLint` (type, c, refs=1103)  
File: `winlib/SDL2-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.829 | `GLint` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.829 | `GLint` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.797 | `GLint64` | type | `...i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 5 | 0.797 | `GLint64` | type | `...6_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 6 | 0.724 | `GLenum` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.724 | `GLenum` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.667 | `GLubyte` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 9 | 0.667 | `GLubyte` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 10 | 0.659 | `GLuint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.8, same_kind=1.0, ns_overlap=0.3, unique_files=6

**Pivot:** `GLint` (type, c, refs=1103)  
File: `winlib/SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLint` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.829 | `GLint` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.829 | `GLint` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.797 | `GLint64` | type | `...i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 5 | 0.797 | `GLint64` | type | `...6_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 6 | 0.724 | `GLenum` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.724 | `GLenum` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.667 | `GLubyte` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 9 | 0.667 | `GLubyte` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 10 | 0.659 | `GLuint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.3, unique_files=6

**Pivot:** `GLenum` (type, c, refs=1093)  
File: `winlib/SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLenum` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 2 | 0.904 | `GLenum` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 3 | 0.904 | `GLenum` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 4 | 0.774 | `t` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 5 | 0.774 | `t` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 6 | 0.774 | `t` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 7 | 0.774 | `t` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 8 | 0.774 | `t` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 9 | 0.774 | `t` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 10 | 0.774 | `t` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=0.3, unique_files=4

### slim_dce0015d (php)

**Pivot:** `handle` (method, php, refs=112)  
File: `Slim/Routing/RouteRunner.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.906 | `handle` | method | `Slim/App.php` |
| 2 | 0.900 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.900 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 4 | 0.900 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 5 | 0.817 | `handle` | method | `Slim/Routing/Route.php` |
| 6 | 0.804 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 7 | 0.691 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |
| 8 | 0.691 | `process` | method | `Slim/Middleware/ContentLengthMiddleware.php` |
| 9 | 0.691 | `process` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 10 | 0.691 | `process` | method | `Slim/Middleware/MethodOverrideMiddleware.php` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.6, unique_files=7

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/MiddlewareDispatcher.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 2 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.974 | `handle` | method | `Slim/App.php` |
| 4 | 0.900 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 5 | 0.876 | `handle` | method | `Slim/Routing/Route.php` |
| 6 | 0.846 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 7 | 0.730 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 8 | 0.730 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |
| 9 | 0.730 | `process` | method | `Slim/Middleware/ContentLengthMiddleware.php` |
| 10 | 0.730 | `process` | method | `Slim/Middleware/ErrorMiddleware.php` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.6, unique_files=7

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/MiddlewareDispatcher.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 2 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.974 | `handle` | method | `Slim/App.php` |
| 4 | 0.900 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 5 | 0.876 | `handle` | method | `Slim/Routing/Route.php` |
| 6 | 0.846 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 7 | 0.730 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 8 | 0.730 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |
| 9 | 0.730 | `process` | method | `Slim/Middleware/ContentLengthMiddleware.php` |
| 10 | 0.730 | `process` | method | `Slim/Middleware/ErrorMiddleware.php` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.6, unique_files=7

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/MiddlewareDispatcher.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.848 | `handle` | method | `Slim/App.php` |
| 2 | 0.846 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.846 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 4 | 0.846 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 5 | 0.804 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 6 | 0.770 | `handle` | method | `Slim/Routing/Route.php` |
| 7 | 0.650 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 8 | 0.639 | `process` | method | `Slim/Middleware/OutputBufferingMiddleware.php` |
| 9 | 0.638 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |
| 10 | 0.638 | `process` | method | `Slim/Middleware/ContentLengthMiddleware.php` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.6, unique_files=8

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/Routing/Route.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.876 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 2 | 0.876 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.876 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 4 | 0.848 | `handle` | method | `Slim/App.php` |
| 5 | 0.817 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 6 | 0.814 | `run` | method | `Slim/Routing/Route.php` |
| 7 | 0.770 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 8 | 0.664 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 9 | 0.636 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |
| 10 | 0.636 | `process` | method | `Slim/Middleware/ContentLengthMiddleware.php` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.6, unique_files=7

### flask_9045020a (python)

**Pivot:** `get` (method, python, refs=386)  
File: `src/flask/sansio/scaffold.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.750 | `put` | method | `src/flask/sansio/scaffold.py` |
| 2 | 0.743 | `delete` | method | `src/flask/sansio/scaffold.py` |
| 3 | 0.693 | `patch` | method | `src/flask/sansio/scaffold.py` |
| 4 | 0.657 | `_method_route` | method | `src/flask/sansio/scaffold.py` |
| 5 | 0.629 | `route` | method | `src/flask/sansio/scaffold.py` |
| 6 | 0.606 | `decorator` | method | `src/flask/sansio/scaffold.py` |
| 7 | 0.604 | `post` | method | `src/flask/sansio/scaffold.py` |
| 8 | 0.497 | `add_url_rule` | method | `src/flask/sansio/app.py` |
| 9 | 0.496 | `add_url_rule` | method | `src/flask/sansio/scaffold.py` |
| 10 | 0.451 | `add_url_rule` | method | `src/flask/sansio/blueprints.py` |

Quality: diversity=0.2, same_kind=1.0, ns_overlap=0.0, unique_files=3

**Pivot:** `get` (method, python, refs=379)  
File: `src/flask/ctx.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.728 | `setdefault` | method | `src/flask/ctx.py` |
| 2 | 0.647 | `pop` | method | `src/flask/ctx.py` |
| 3 | 0.495 | `__getattr__` | function | `src/flask/ctx.py` |
| 4 | 0.495 | `__getattr__` | function | `src/flask/globals.py` |
| 5 | 0.423 | `__setattr__` | method | `src/flask/ctx.py` |
| 6 | 0.414 | `get` | method | `src/flask/sansio/scaffold.py` |
| 7 | 0.402 | `__delattr__` | method | `src/flask/ctx.py` |
| 8 | 0.400 | `get_template_attribute` | function | `src/flask/helpers.py` |
| 9 | 0.390 | `attr` | variable | `src/flask/cli.py` |
| 10 | 0.384 | `get_command` | method | `src/flask/cli.py` |

Quality: diversity=0.5, same_kind=0.6, ns_overlap=0.1, unique_files=5

**Pivot:** `route` (method, python, refs=288)  
File: `src/flask/sansio/scaffold.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.731 | `endpoint` | method | `src/flask/sansio/scaffold.py` |
| 2 | 0.717 | `decorator` | method | `src/flask/sansio/scaffold.py` |
| 3 | 0.684 | `add_url_rule` | method | `src/flask/sansio/scaffold.py` |
| 4 | 0.658 | `add_url_rule` | method | `src/flask/sansio/app.py` |
| 5 | 0.645 | `add_url_rule` | method | `src/flask/sansio/blueprints.py` |
| 6 | 0.645 | `_method_route` | method | `src/flask/sansio/scaffold.py` |
| 7 | 0.639 | `add_url_rule` | method | `src/flask/sansio/blueprints.py` |
| 8 | 0.631 | `delete` | method | `src/flask/sansio/scaffold.py` |
| 9 | 0.629 | `get` | method | `src/flask/sansio/scaffold.py` |
| 10 | 0.623 | `put` | method | `src/flask/sansio/scaffold.py` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.0, unique_files=3

**Pivot:** `Flask` (class, python, refs=125)  
File: `src/flask/app.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.767 | `App` | class | `src/flask/sansio/app.py` |
| 2 | 0.614 | `FlaskClient` | class | `src/flask/testing.py` |
| 3 | 0.610 | `FlaskTask` | class | `examples/celery/src/task_app/__init__.py` |
| 4 | 0.572 | `FlaskCliRunner` | class | `src/flask/testing.py` |
| 5 | 0.569 | `Scaffold` | class | `src/flask/sansio/scaffold.py` |
| 6 | 0.562 | `FlaskGroup` | class | `src/flask/cli.py` |
| 7 | 0.540 | `Request` | class | `src/flask/wrappers.py` |
| 8 | 0.519 | `ScriptInfo` | class | `src/flask/cli.py` |
| 9 | 0.503 | `AppContext` | class | `src/flask/ctx.py` |
| 10 | 0.481 | `FlaskProxy` | class | `src/flask/globals.py` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.0, unique_files=8

**Pivot:** `jinja_env` (method, python, refs=124)  
File: `src/flask/sansio/app.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.666 | `jinja_loader` | method | `src/flask/sansio/scaffold.py` |
| 2 | 0.637 | `create_jinja_environment` | method | `src/flask/sansio/app.py` |
| 3 | 0.590 | `create_jinja_environment` | method | `src/flask/app.py` |
| 4 | 0.574 | `Environment` | class | `src/flask/templating.py` |
| 5 | 0.412 | `create_global_jinja_loader` | method | `src/flask/sansio/app.py` |
| 6 | 0.378 | `logger` | method | `src/flask/sansio/app.py` |
| 7 | 0.368 | `template_global` | method | `src/flask/sansio/app.py` |
| 8 | 0.359 | `app_template_global` | method | `src/flask/sansio/blueprints.py` |
| 9 | 0.356 | `_copy_environ` | method | `src/flask/testing.py` |
| 10 | 0.327 | `template_filter` | method | `src/flask/sansio/app.py` |

Quality: diversity=0.5, same_kind=0.9, ns_overlap=0.0, unique_files=6

### sinatra_86eed2fe (ruby)

**Pivot:** `get` (method, ruby, refs=1210)  
File: `sinatra-contrib/lib/sinatra/multi_route.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.770 | `route` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 2 | 0.738 | `delete` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 3 | 0.735 | `put` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 4 | 0.726 | `options` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 5 | 0.694 | `patch` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 6 | 0.694 | `head` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 7 | 0.672 | `route_args` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 8 | 0.647 | `post` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 9 | 0.588 | `MultiRoute` | module | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 10 | 0.573 | `get` | method | `lib/sinatra/base.rb` |

Quality: diversity=0.1, same_kind=0.9, ns_overlap=0.1, unique_files=2

**Pivot:** `get` (method, ruby, refs=1209)  
File: `lib/sinatra/base.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.576 | `delete` | method | `lib/sinatra/base.rb` |
| 2 | 0.573 | `get` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 3 | 0.571 | `options` | method | `lib/sinatra/base.rb` |
| 4 | 0.546 | `get` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 5 | 0.541 | `put` | method | `lib/sinatra/base.rb` |
| 6 | 0.503 | `link` | method | `lib/sinatra/base.rb` |
| 7 | 0.487 | `patch` | method | `lib/sinatra/base.rb` |
| 8 | 0.472 | `route` | method | `lib/sinatra/base.rb` |
| 9 | 0.465 | `head` | method | `lib/sinatra/base.rb` |
| 10 | 0.447 | `unlink` | method | `lib/sinatra/base.rb` |

Quality: diversity=0.2, same_kind=1.0, ns_overlap=0.2, unique_files=3

**Pivot:** `get` (method, ruby, refs=1209)  
File: `sinatra-contrib/lib/sinatra/runner.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.770 | `get_url` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 2 | 0.721 | `get_response` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 3 | 0.546 | `get` | method | `lib/sinatra/base.rb` |
| 4 | 0.533 | `get_stream` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 5 | 0.465 | `get` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 6 | 0.407 | `link` | method | `sinatra-contrib/lib/sinatra/link_header.rb` |
| 7 | 0.395 | `run` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 8 | 0.376 | `log` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 9 | 0.376 | `get_https_url` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 10 | 0.371 | `start` | method | `sinatra-contrib/lib/sinatra/runner.rb` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.2, unique_files=4

**Pivot:** `to` (method, ruby, refs=680)  
File: `lib/sinatra/base.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.892 | `url` | method | `lib/sinatra/base.rb` |
| 2 | 0.687 | `status` | method | `lib/sinatra/base.rb` |
| 3 | 0.574 | `with_params` | method | `lib/sinatra/base.rb` |
| 4 | 0.472 | `http_status` | method | `lib/sinatra/base.rb` |
| 5 | 0.472 | `http_status` | method | `lib/sinatra/base.rb` |
| 6 | 0.427 | `redirect_to` | method | `sinatra-contrib/lib/sinatra/namespace.rb` |
| 7 | 0.396 | `http_status` | variable | `lib/sinatra/base.rb` |
| 8 | 0.396 | `uri` | method | `lib/sinatra/base.rb` |
| 9 | 0.380 | `methodoverride=` | method | `lib/sinatra/base.rb` |
| 10 | 0.378 | `@response.status` | variable | `lib/sinatra/base.rb` |

Quality: diversity=0.1, same_kind=0.8, ns_overlap=0.0, unique_files=2

**Pivot:** `new` (method, ruby, refs=326)  
File: `rack-protection/lib/rack/protection.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.631 | `new` | method | `lib/sinatra/base.rb` |
| 2 | 0.626 | `new!` | method | `lib/sinatra/base.rb` |
| 3 | 0.596 | `new` | method | `lib/sinatra/base.rb` |
| 4 | 0.572 | `new` | method | `sinatra-contrib/lib/sinatra/extension.rb` |
| 5 | 0.547 | `app` | method | `sinatra-contrib/lib/sinatra/namespace.rb` |
| 6 | 0.544 | `new` | method | `sinatra-contrib/lib/sinatra/namespace.rb` |
| 7 | 0.440 | `base` | variable | `lib/sinatra/base.rb` |
| 8 | 0.439 | `build` | method | `lib/sinatra/base.rb` |
| 9 | 0.436 | `default` | method | `sinatra-contrib/lib/sinatra/cookies.rb` |
| 10 | 0.434 | `for` | method | `sinatra-contrib/lib/sinatra/reloader.rb` |

Quality: diversity=1.0, same_kind=0.9, ns_overlap=0.4, unique_files=5

### julie_316c0b08 (rust)

**Pivot:** `name` (method, rust, refs=3397)  
File: `src/tools/metrics/session.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.766 | `as_str` | method | `src/embeddings/mod.rs` |
| 2 | 0.673 | `new` | method | `src/tools/workspace/parser_pool.rs` |
| 3 | 0.673 | `new` | method | `src/utils/token_estimation.rs` |
| 4 | 0.673 | `new` | method | `crates/julie-extractors/src/manager.rs` |
| 5 | 0.673 | `new` | method | `src/tools/metrics/session.rs` |
| 6 | 0.673 | `new` | method | `src/utils/context_truncation.rs` |
| 7 | 0.606 | `current` | method | `src/embeddings/factory.rs` |
| 8 | 0.579 | `len` | method | `src/search/language_config.rs` |
| 9 | 0.547 | `display_name` | method | `fixtures/test-workspaces/tiny-primary/src/lib.rs` |
| 10 | 0.531 | `new` | method | `src/tracing/mod.rs` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.0, unique_files=10

**Pivot:** `new` (method, rust, refs=3196)  
File: `src/search/schema.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.618 | `new` | method | `src/database/mod.rs` |
| 2 | 0.579 | `new` | method | `src/tools/workspace/parser_pool.rs` |
| 3 | 0.579 | `new` | method | `src/utils/token_estimation.rs` |
| 4 | 0.579 | `new` | method | `crates/julie-extractors/src/manager.rs` |
| 5 | 0.579 | `new` | method | `src/tools/metrics/session.rs` |
| 6 | 0.579 | `new` | method | `src/utils/context_truncation.rs` |
| 7 | 0.566 | `new` | method | `src/tracing/mod.rs` |
| 8 | 0.539 | `new` | method | `src/search/tokenizer.rs` |
| 9 | 0.532 | `new` | method | `src/utils/cross_language_intelligence.rs` |
| 10 | 0.519 | `new` | method | `crates/julie-extractors/src/json/mod.rs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=10

**Pivot:** `new` (method, rust, refs=3196)  
File: `src/tools/get_context/allocation.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.617 | `TokenBudget` | struct | `src/tools/get_context/allocation.rs` |
| 2 | 0.571 | `new` | method | `crates/julie-extractors/src/manager.rs` |
| 3 | 0.571 | `new` | method | `src/tools/metrics/session.rs` |
| 4 | 0.571 | `new` | method | `src/utils/context_truncation.rs` |
| 5 | 0.571 | `new` | method | `src/tools/workspace/parser_pool.rs` |
| 6 | 0.571 | `new` | method | `src/utils/token_estimation.rs` |
| 7 | 0.551 | `new` | method | `crates/julie-extractors/src/json/mod.rs` |
| 8 | 0.551 | `new` | method | `crates/julie-extractors/src/markdown/mod.rs` |
| 9 | 0.551 | `new` | method | `crates/julie-extractors/src/toml/mod.rs` |
| 10 | 0.551 | `new` | method | `crates/julie-extractors/src/yaml/mod.rs` |

Quality: diversity=0.9, same_kind=0.9, ns_overlap=0.9, unique_files=10

**Pivot:** `new` (method, rust, refs=3195)  
File: `src/utils/token_estimation.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `new` | method | `src/tools/workspace/parser_pool.rs` |
| 2 | 1.000 | `new` | method | `crates/julie-extractors/src/manager.rs` |
| 3 | 1.000 | `new` | method | `src/tools/metrics/session.rs` |
| 4 | 1.000 | `new` | method | `src/utils/context_truncation.rs` |
| 5 | 0.803 | `new` | method | `src/workspace/mod.rs` |
| 6 | 0.776 | `new` | method | `src/tracing/mod.rs` |
| 7 | 0.751 | `new` | method | `fixtures/test-workspaces/tiny-primary/src/lib.rs` |
| 8 | 0.749 | `new` | method | `crates/julie-extractors/src/json/mod.rs` |
| 9 | 0.749 | `new` | method | `crates/julie-extractors/src/markdown/mod.rs` |
| 10 | 0.749 | `new` | method | `crates/julie-extractors/src/toml/mod.rs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=10

**Pivot:** `new` (method, rust, refs=3195)  
File: `src/database/mod.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.658 | `new` | method | `src/tracing/mod.rs` |
| 2 | 0.618 | `new` | method | `crates/julie-extractors/src/bash/mod.rs` |
| 3 | 0.618 | `new` | method | `crates/julie-extractors/src/javascript/mod.rs` |
| 4 | 0.618 | `new` | method | `crates/julie-extractors/src/lua/mod.rs` |
| 5 | 0.618 | `new` | method | `crates/julie-extractors/src/php/mod.rs` |
| 6 | 0.618 | `new` | method | `crates/julie-extractors/src/powershell/mod.rs` |
| 7 | 0.618 | `new` | method | `crates/julie-extractors/src/qml/mod.rs` |
| 8 | 0.618 | `new` | method | `crates/julie-extractors/src/r/mod.rs` |
| 9 | 0.618 | `new` | method | `crates/julie-extractors/src/css/mod.rs` |
| 10 | 0.618 | `new` | method | `crates/julie-extractors/src/dart/mod.rs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=10

### cats_c701f713 (scala)

**Pivot:** `*` (method, scala, refs=2255)  
File: `algebra-core/src/main/scala/algebra/ring/Signed.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.951 | `**` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 2 | 0.817 | `unary_-` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 3 | 0.646 | `Sign` | class | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 4 | 0.594 | `one` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 5 | 0.588 | `sign` | method | `...a-2.12/cats/kernel/compat/scalaVersionSpecific.scala` |
| 6 | 0.572 | `sign` | method | `...a-2.12/cats/kernel/compat/scalaVersionSpecific.scala` |
| 7 | 0.565 | `Sign` | class | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 8 | 0.560 | `abs` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 9 | 0.547 | `Positive` | class | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 10 | 0.546 | `sign` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |

Quality: diversity=0.2, same_kind=0.7, ns_overlap=0.0, unique_files=2

**Pivot:** `*` (method, scala, refs=2255)  
File: `laws/src/main/scala/cats/laws/discipline/MiniInt.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.987 | `|` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 2 | 0.975 | `/` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 3 | 0.944 | `+` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 4 | 0.797 | `unary_-` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 5 | 0.687 | `MiniInt` | class | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 6 | 0.651 | `MiniInt` | class | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 7 | 0.617 | `toInt` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 8 | 0.572 | `wrapped` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 9 | 0.563 | `fromInt` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 10 | 0.559 | `unsafeFromInt` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |

Quality: diversity=0.0, same_kind=0.8, ns_overlap=0.0, unique_files=1

**Pivot:** `map` (class, scala, refs=1472)  
File: `core/src/main/scala-2.12/cats/instances/package.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `map` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 2 | 0.752 | `map` | class | `alleycats-core/src/main/scala/alleycats/std/map.scala` |
| 3 | 0.696 | `option` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 4 | 0.696 | `option` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 5 | 0.668 | `stream` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 6 | 0.662 | `all` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 7 | 0.662 | `all` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 8 | 0.653 | `function` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 9 | 0.653 | `function` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 10 | 0.647 | `stream` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=0.2, unique_files=3

**Pivot:** `map` (method, scala, refs=1472)  
File: `core/src/main/scala/cats/data/AndThen.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.851 | `map` | method | `core/src/main/scala/cats/data/OneAnd.scala` |
| 2 | 0.851 | `map` | method | `core/src/main/scala/cats/data/OneAnd.scala` |
| 3 | 0.821 | `map` | method | `core/src/main/scala/cats/package.scala` |
| 4 | 0.819 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 5 | 0.811 | `map` | method | `core/src/main/scala/cats/data/OneAnd.scala` |
| 6 | 0.809 | `map` | method | `core/src/main/scala/cats/instances/function.scala` |
| 7 | 0.806 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 8 | 0.806 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 9 | 0.789 | `map` | method | `...main/scala/cats/data/IndexedReaderWriterStateT.scala` |
| 10 | 0.780 | `map` | method | `free/src/main/scala/cats/free/FreeApplicative.scala` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=6

**Pivot:** `map` (method, scala, refs=1472)  
File: `core/src/main/scala/cats/Bifunctor.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.929 | `map` | method | `core/src/main/scala/cats/Bifunctor.scala` |
| 2 | 0.895 | `map` | method | `core/src/main/scala/cats/data/Func.scala` |
| 3 | 0.888 | `map` | method | `core/src/main/scala/cats/Applicative.scala` |
| 4 | 0.871 | `map` | method | `alleycats-core/src/main/scala/alleycats/Pure.scala` |
| 5 | 0.871 | `map` | method | `alleycats-core/src/main/scala/alleycats/Extract.scala` |
| 6 | 0.871 | `map` | method | `core/src/main/scala/cats/Parallel.scala` |
| 7 | 0.871 | `map` | method | `core/src/main/scala/cats/Representable.scala` |
| 8 | 0.814 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 9 | 0.814 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 10 | 0.806 | `map` | method | `core/src/main/scala/cats/data/Const.scala` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=1.0, unique_files=9

### alamofire_3d4cceb5 (swift)

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.954 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.944 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.934 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.875 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.822 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.807 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.803 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.764 | `request` | method | `Source/Features/EventMonitor.swift` |
| 9 | 0.752 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.750 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.964 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.957 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.954 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.878 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.831 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.825 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.812 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.791 | `request` | method | `Source/Features/EventMonitor.swift` |
| 9 | 0.785 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.774 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.964 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.944 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.934 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.831 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.812 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.805 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.804 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.795 | `request` | method | `Source/Features/EventMonitor.swift` |
| 9 | 0.786 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.781 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.957 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.944 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.944 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.861 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.821 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.810 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.797 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.764 | `request` | method | `Source/Features/EventMonitor.swift` |
| 9 | 0.759 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.749 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `request` (method, swift, refs=544)  
File: `Source/Core/Session.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.921 | `request` | method | `Source/Core/Session.swift` |
| 2 | 0.914 | `request` | method | `Source/Core/Session.swift` |
| 3 | 0.747 | `streamRequest` | method | `Source/Core/Session.swift` |
| 4 | 0.732 | `streamRequest` | method | `Source/Core/Session.swift` |
| 5 | 0.650 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.647 | `streamRequest` | method | `Source/Core/Session.swift` |
| 7 | 0.638 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.632 | `request` | method | `Source/Features/EventMonitor.swift` |
| 9 | 0.630 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.627 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.5, same_kind=1.0, ns_overlap=0.7, unique_files=2

### labhandbookv2_67e8c1cf (typescript)

**Pivot:** `Error` (method, csharp, refs=84)  
File: `src/LabHandbook.Api/Models/Dto/ApiResponse.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.828 | `Error` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 2 | 0.644 | `ApiError` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 3 | 0.635 | `ApiError` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 4 | 0.493 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 5 | 0.493 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 6 | 0.489 | `FieldError` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 7 | 0.478 | `ApiResponse` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 8 | 0.462 | `FieldError` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 9 | 0.459 | `error` | function | `...abHandbook.Api/ClientApp/src/stores/notifications.ts` |
| 10 | 0.445 | `Success` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |

Quality: diversity=0.4, same_kind=0.2, ns_overlap=0.1, unique_files=3

**Pivot:** `Error` (method, csharp, refs=84)  
File: `src/LabHandbook.Api/Models/Dto/ApiResponse.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.828 | `Error` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 2 | 0.670 | `ApiError` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 3 | 0.649 | `ApiError` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 4 | 0.564 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 5 | 0.552 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 6 | 0.511 | `Success` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 7 | 0.511 | `ApiResponse` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 8 | 0.423 | `error` | function | `...abHandbook.Api/ClientApp/src/stores/notifications.ts` |
| 9 | 0.418 | `Success` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 10 | 0.392 | `WriteErrorResponse` | method | `...Infrastructure/Middleware/ErrorHandlingMiddleware.cs` |

Quality: diversity=0.4, same_kind=0.4, ns_overlap=0.1, unique_files=4

**Pivot:** `ToDto` (method, csharp, refs=56)  
File: `src/LabHandbook.Api/Models/Mapping/SectionMappingExtensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.849 | `ToDto` | method | `...dbook.Api/Models/Mapping/LabTestMappingExtensions.cs` |
| 2 | 0.705 | `SectionDto` | class | `src/LabHandbook.Api/Models/Dto/SectionDto.cs` |
| 3 | 0.675 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 4 | 0.653 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 5 | 0.622 | `SectionDto` | interface | `src/LabHandbook.Api/ClientApp/src/types/pages.ts` |
| 6 | 0.612 | `ToDto` | method | `...dbook.Api/Models/Mapping/ContentMappingExtensions.cs` |
| 7 | 0.609 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 8 | 0.608 | `ToDto` | method | `...book.Api/Models/Mapping/CalendarMappingExtensions.cs` |
| 9 | 0.591 | `ToDto` | method | `...andbook.Api/Models/Mapping/MediaMappingExtensions.cs` |
| 10 | 0.590 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |

Quality: diversity=1.0, same_kind=0.8, ns_overlap=0.8, unique_files=8

**Pivot:** `ToDto` (method, csharp, refs=56)  
File: `src/LabHandbook.Api/Models/Mapping/PageMappingExtensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.821 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 2 | 0.661 | `PageLinkDto` | class | `src/LabHandbook.Api/Models/Dto/PageLinkDto.cs` |
| 3 | 0.638 | `deleteLink` | function | `...Api/ClientApp/src/components/cms/CmsDocumentList.vue` |
| 4 | 0.638 | `deleteLink` | function | `...ook.Api/ClientApp/src/components/cms/CmsLinkList.vue` |
| 5 | 0.618 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 6 | 0.605 | `PageLinkDto` | interface | `src/LabHandbook.Api/ClientApp/src/types/pages.ts` |
| 7 | 0.596 | `ToDto` | method | `...dbook.Api/Models/Mapping/ContentMappingExtensions.cs` |
| 8 | 0.590 | `ToDto` | method | `...dbook.Api/Models/Mapping/SectionMappingExtensions.cs` |
| 9 | 0.587 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 10 | 0.584 | `ToDto` | method | `...book.Api/Models/Mapping/CalendarMappingExtensions.cs` |

Quality: diversity=0.9, same_kind=0.6, ns_overlap=0.6, unique_files=9

**Pivot:** `ToDto` (method, csharp, refs=56)  
File: `src/LabHandbook.Api/Models/Mapping/MediaMappingExtensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.651 | `MediaFileDto` | class | `src/LabHandbook.Api/Models/Dto/MediaFileDto.cs` |
| 2 | 0.618 | `MediaFileDto` | interface | `src/LabHandbook.Api/ClientApp/src/types/media.ts` |
| 3 | 0.600 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 4 | 0.591 | `ToDto` | method | `...dbook.Api/Models/Mapping/SectionMappingExtensions.cs` |
| 5 | 0.578 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 6 | 0.578 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 7 | 0.564 | `onEditFile` | function | `...ok.Api/ClientApp/src/components/admin/MediaAdmin.vue` |
| 8 | 0.563 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 9 | 0.561 | `ToDto` | method | `...book.Api/Models/Mapping/CalendarMappingExtensions.cs` |
| 10 | 0.551 | `ToDto` | method | `...dbook.Api/Models/Mapping/ContentMappingExtensions.cs` |

Quality: diversity=1.0, same_kind=0.7, ns_overlap=0.7, unique_files=8

### zod_df52de88 (typescript)

**Pivot:** `parse` (method, typescript, refs=3156)  
File: `packages/bench/safeparse.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.736 | `$Parse` | type | `packages/zod/src/v4/core/parse.ts` |
| 2 | 0.700 | `safeparse` | method | `packages/bench/safeparse.ts` |
| 3 | 0.659 | `parse` | method | `packages/zod/src/v3/types.ts` |
| 4 | 0.638 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 5 | 0.632 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 6 | 0.632 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 7 | 0.632 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 8 | 0.632 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 9 | 0.620 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 10 | 0.620 | `_parse` | method | `packages/zod/src/v3/types.ts` |

Quality: diversity=0.9, same_kind=0.9, ns_overlap=0.1, unique_files=3

**Pivot:** `parse` (method, typescript, refs=3154)  
File: `packages/zod/src/v4/mini/schemas.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.975 | `parse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 2 | 0.820 | `safeParse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 3 | 0.819 | `parseAsync` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 4 | 0.819 | `parseAsync` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 5 | 0.797 | `safeParse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 6 | 0.757 | `decode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 7 | 0.693 | `parse` | method | `packages/zod/src/v3/types.ts` |
| 8 | 0.679 | `encode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 9 | 0.607 | `decodeAsync` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 10 | 0.562 | `parse` | method | `packages/bench/safeparse.ts` |

Quality: diversity=0.8, same_kind=1.0, ns_overlap=0.3, unique_files=4

**Pivot:** `parse` (method, typescript, refs=3153)  
File: `packages/zod/src/v4/classic/schemas.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.975 | `parse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 2 | 0.805 | `safeParse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 3 | 0.795 | `parseAsync` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 4 | 0.795 | `parseAsync` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 5 | 0.781 | `safeParse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 6 | 0.729 | `decode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 7 | 0.681 | `parse` | method | `packages/zod/src/v3/types.ts` |
| 8 | 0.662 | `encode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 9 | 0.609 | `parse` | method | `packages/bench/safeparse.ts` |
| 10 | 0.578 | `decodeAsync` | method | `packages/zod/src/v4/classic/schemas.ts` |

Quality: diversity=0.5, same_kind=1.0, ns_overlap=0.3, unique_files=4

**Pivot:** `parse` (method, typescript, refs=3152)  
File: `packages/zod/src/v3/types.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.732 | `parseAsync` | method | `packages/zod/src/v3/types.ts` |
| 2 | 0.721 | `safeParse` | method | `packages/zod/src/v3/types.ts` |
| 3 | 0.693 | `parse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 4 | 0.681 | `parse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 5 | 0.659 | `parse` | method | `packages/bench/safeparse.ts` |
| 6 | 0.592 | `safeParse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 7 | 0.571 | `safeParse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 8 | 0.567 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 9 | 0.567 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 10 | 0.567 | `_parse` | method | `packages/zod/src/v3/types.ts` |

Quality: diversity=0.5, same_kind=1.0, ns_overlap=0.3, unique_files=4

**Pivot:** `parse` (method, typescript, refs=3102)  
File: `packages/zod/src/v4/core/schemas.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.743 | `run` | method | `packages/zod/src/v4/core/schemas.ts` |
| 2 | 0.606 | `check` | method | `packages/zod/src/v4/core/checks.ts` |
| 3 | 0.604 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 4 | 0.604 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 5 | 0.604 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 6 | 0.604 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 7 | 0.568 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 8 | 0.554 | `parse` | method | `packages/bench/safeparse.ts` |
| 9 | 0.553 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 10 | 0.553 | `_parse` | method | `packages/zod/src/v3/types.ts` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.1, unique_files=4

### zls_4b29ec8b (zig)

**Pivot:** `allocator` (method, zig, refs=690)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.885 | `allocator` | method | `src/testing.zig` |
| 2 | 0.661 | `init` | method | `src/tracy.zig` |
| 3 | 0.601 | `tracyAllocator` | function | `src/tracy.zig` |
| 4 | 0.586 | `addOne` | method | `src/analyser/segmented_list.zig` |
| 5 | 0.586 | `append` | method | `src/analyser/segmented_list.zig` |
| 6 | 0.560 | `iterator` | method | `src/analyser/segmented_list.zig` |
| 7 | 0.545 | `push` | method | `src/analysis.zig` |
| 8 | 0.535 | `init` | method | `src/testing.zig` |
| 9 | 0.525 | `at` | method | `src/analyser/segmented_list.zig` |
| 10 | 0.520 | `TracyAllocator` | function | `src/tracy.zig` |

Quality: diversity=0.7, same_kind=0.8, ns_overlap=0.1, unique_files=4

**Pivot:** `end` (method, zig, refs=485)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `end` | method | `src/tracy.zig` |
| 2 | 0.863 | `end` | method | `src/tracy.zig` |
| 3 | 0.498 | `setValue` | method | `src/tracy.zig` |
| 4 | 0.498 | `setValue` | method | `src/tracy.zig` |
| 5 | 0.495 | `append` | method | `src/ast.zig` |
| 6 | 0.485 | `count` | method | `src/analyser/segmented_list.zig` |
| 7 | 0.477 | `addText` | method | `src/tracy.zig` |
| 8 | 0.477 | `addText` | method | `src/tracy.zig` |
| 9 | 0.477 | `main` | function | `.github/workflows/prepare_release_payload.zig` |
| 10 | 0.465 | `append` | method | `src/analyser/segmented_list.zig` |

Quality: diversity=0.4, same_kind=0.9, ns_overlap=0.2, unique_files=4

**Pivot:** `end` (method, zig, refs=485)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `end` | method | `src/tracy.zig` |
| 2 | 0.863 | `end` | method | `src/tracy.zig` |
| 3 | 0.498 | `setValue` | method | `src/tracy.zig` |
| 4 | 0.498 | `setValue` | method | `src/tracy.zig` |
| 5 | 0.495 | `append` | method | `src/ast.zig` |
| 6 | 0.485 | `count` | method | `src/analyser/segmented_list.zig` |
| 7 | 0.477 | `addText` | method | `src/tracy.zig` |
| 8 | 0.477 | `addText` | method | `src/tracy.zig` |
| 9 | 0.477 | `main` | function | `.github/workflows/prepare_release_payload.zig` |
| 10 | 0.465 | `append` | method | `src/analyser/segmented_list.zig` |

Quality: diversity=0.4, same_kind=0.9, ns_overlap=0.2, unique_files=4

**Pivot:** `end` (method, zig, refs=485)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.863 | `end` | method | `src/tracy.zig` |
| 2 | 0.863 | `end` | method | `src/tracy.zig` |
| 3 | 0.554 | `append` | method | `src/ast.zig` |
| 4 | 0.546 | `count` | method | `src/analyser/segmented_list.zig` |
| 5 | 0.544 | `main` | function | `.github/workflows/prepare_release_payload.zig` |
| 6 | 0.537 | `pop` | method | `src/analyser/segmented_list.zig` |
| 7 | 0.509 | `allocator` | method | `src/tracy.zig` |
| 8 | 0.501 | `next` | method | `src/analyser/segmented_list.zig` |
| 9 | 0.497 | `next` | method | `src/analysis.zig` |
| 10 | 0.490 | `append` | method | `src/analyser/segmented_list.zig` |

Quality: diversity=0.7, same_kind=0.9, ns_overlap=0.2, unique_files=5

**Pivot:** `Index` (enum, zig, refs=430)  
File: `src/TrigramStore.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `Index` | enum | `src/analyser/InternPool.zig` |
| 2 | 1.000 | `Index` | enum | `src/analyser/InternPool.zig` |
| 3 | 1.000 | `Index` | enum | `src/analyser/InternPool.zig` |
| 4 | 0.929 | `Index` | enum | `src/DocumentScope.zig` |
| 5 | 0.929 | `Index` | enum | `src/analyser/InternPool.zig` |
| 6 | 0.878 | `Index` | enum | `src/DocumentScope.zig` |
| 7 | 0.763 | `NamespaceIndex` | enum | `src/analyser/InternPool.zig` |
| 8 | 0.732 | `BucketIndex` | enum | `src/TrigramStore.zig` |
| 9 | 0.723 | `OptionalIndex` | enum | `src/analyser/InternPool.zig` |
| 10 | 0.699 | `OptionalIndex` | enum | `src/DocumentScope.zig` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.6, unique_files=3
