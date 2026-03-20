# Julie Embedding Quality Benchmark

**Model:** `bge-small-en-v1.5` (384 dimensions)
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
| julie_316c0b08 | rust | 53968 | 7821 | 14.5% | 82.4% | 0 | 193 | 969 |
| cats_c701f713 | scala | 22336 | 13769 | 61.6% | 69.0% | 149 | 4009 | 0 |
| alamofire_3d4cceb5 | swift | 20555 | 2358 | 11.5% | 31.2% | 0 | 134 | 5142 |
| labhandbookv2_67e8c1cf | typescript | 7306 | 1015 | 13.9% | 25.5% | 1511 | 51 | 383 |
| zod_df52de88 | typescript | 17055 | 2949 | 17.3% | 31.8% | 5533 | 1729 | 2329 |
| zls_4b29ec8b | zig | 10677 | 1588 | 14.9% | 20.9% | 1726 | 4394 | 1533 |

## 2. Quality Summary

| Workspace | Language | Pivots | Avg Top Sim | Avg Diversity | Avg NS Overlap | Cross-lang |
|-----------|----------|--------|-------------|---------------|----------------|------------|
| jq_13566b9e | c | 5/5 | 0.953 | 0.64 | 0.08 | no |
| nlohmann-json_a5f86cd4 | cpp | 5/5 | 0.914 | 0.48 | 0.28 | no |
| newtonsoft_json_afe705a1 | csharp | 5/5 | 0.92 | 0.74 | 0.2 | no |
| riverpod_a7fdc041 | dart | 5/5 | 0.907 | 0.68 | 0.28 | no |
| phoenix_ac16deb4 | elixir | 5/5 | 0.879 | 0.88 | 0.36 | yes |
| cobra_8b201fd3 | go | 5/5 | 0.812 | 0.42 | 0.0 | no |
| guava_7e9af99a | java | 5/5 | 1.0 | 0.44 | 0.78 | no |
| express_8cefd559 | javascript | 5/5 | 0.951 | 0.52 | 0.22 | no |
| moshi_c9c5a600 | kotlin | 5/5 | 0.937 | 0.88 | 0.84 | yes |
| lite_f7e95a20 | lua | 5/5 | 1.0 | 0.7 | 0.34 | no |
| slim_dce0015d | php | 5/5 | 0.974 | 0.8 | 0.6 | no |
| flask_9045020a | python | 5/5 | 0.871 | 0.5 | 0.0 | no |
| sinatra_86eed2fe | ruby | 5/5 | 0.907 | 0.48 | 0.16 | no |
| julie_316c0b08 | rust | 5/5 | 0.876 | 0.86 | 0.6 | no |
| cats_c701f713 | scala | 5/5 | 0.973 | 0.58 | 0.44 | no |
| alamofire_3d4cceb5 | swift | 5/5 | 0.973 | 0.5 | 0.78 | no |
| labhandbookv2_67e8c1cf | typescript | 5/5 | 0.907 | 0.64 | 0.38 | yes |
| zod_df52de88 | typescript | 5/5 | 0.912 | 0.68 | 0.16 | no |
| zls_4b29ec8b | zig | 5/5 | 0.982 | 0.74 | 0.26 | no |

## 3. Aggregate Metrics

- **Total pivot queries:** 95
- **Avg top similarity:** 0.929
- **Avg mean similarity:** 0.849
- **Avg diversity (cross-file):** 0.64
- **Avg namespace overlap:** 0.356
- **Avg same-kind ratio:** 0.839
- **Cross-language results:** 10.5%

## 4. Detailed Results

### jq_13566b9e (c)

**Pivot:** `jv` (type, c, refs=1134)  
File: `src/jq.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.942 | `jv_kind` | type | `src/jv.h` |
| 2 | 0.933 | `jv_parser` | struct | `src/jv.h` |
| 3 | 0.894 | `jq_state` | struct | `src/jq.h` |
| 4 | 0.886 | `jq_state` | type | `src/jq.h` |
| 5 | 0.865 | `jq_util_input_state` | struct | `src/jq.h` |
| 6 | 0.858 | `jv_nomem_handler_f` | type | `src/jv.h` |
| 7 | 0.853 | `jv_parser` | struct | `src/jv.h` |
| 8 | 0.853 | `jv_parser` | struct | `src/jv_file.c` |
| 9 | 0.853 | `jv_parser` | struct | `src/jv_parse.c` |
| 10 | 0.853 | `jv_parser` | struct | `src/jv_parse.c` |

Quality: diversity=0.7, same_kind=0.3, ns_overlap=0.0, unique_files=4

**Pivot:** `jv` (union, c, refs=1003)  
File: `src/jv.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.853 | `jv` | type | `src/jq.h` |
| 2 | 0.837 | `jv_parser` | struct | `src/jv.h` |
| 3 | 0.829 | `jq_util_input_state` | struct | `src/jq.h` |
| 4 | 0.824 | `jq_state` | struct | `src/jq.h` |
| 5 | 0.818 | `jvp_object` | struct | `src/jv.c` |
| 6 | 0.818 | `jvp_string` | struct | `src/jv.c` |
| 7 | 0.815 | `jv_kind` | type | `src/jv.h` |
| 8 | 0.810 | `U` | union | `src/jv_dtoa.c` |
| 9 | 0.807 | `YYSTYPE` | union | `src/parser.c` |
| 10 | 0.807 | `YYSTYPE` | union | `src/parser.h` |

Quality: diversity=0.8, same_kind=0.3, ns_overlap=0.1, unique_files=6

**Pivot:** `jv_free` (function, c, refs=605)  
File: `src/jv.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.997 | `jv_free` | function | `src/jv.c` |
| 2 | 0.938 | `jv_mem_free` | function | `src/jv_alloc.h` |
| 3 | 0.920 | `jvp_number_free` | function | `src/jv.c` |
| 4 | 0.917 | `jv_mem_free` | function | `src/jv_alloc.c` |
| 5 | 0.917 | `jvp_object_free` | function | `src/jv.c` |
| 6 | 0.916 | `jvp_string_free` | function | `src/jv.c` |
| 7 | 0.915 | `jv_parser_free` | function | `src/jv.h` |
| 8 | 0.903 | `jvp_invalid_free` | function | `src/jv.c` |
| 9 | 0.898 | `jvp_array_free` | function | `src/jv.c` |
| 10 | 0.878 | `jv_parser_free` | function | `src/jv_parse.c` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.1, unique_files=5

**Pivot:** `jv_free` (function, c, refs=549)  
File: `src/jv.c`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.997 | `jv_free` | function | `src/jv.h` |
| 2 | 0.938 | `jv_mem_free` | function | `src/jv_alloc.h` |
| 3 | 0.921 | `jvp_number_free` | function | `src/jv.c` |
| 4 | 0.920 | `jv_mem_free` | function | `src/jv_alloc.c` |
| 5 | 0.916 | `jvp_object_free` | function | `src/jv.c` |
| 6 | 0.913 | `jvp_string_free` | function | `src/jv.c` |
| 7 | 0.912 | `jv_parser_free` | function | `src/jv.h` |
| 8 | 0.901 | `jvp_invalid_free` | function | `src/jv.c` |
| 9 | 0.895 | `jvp_array_free` | function | `src/jv.c` |
| 10 | 0.878 | `jv_parser_free` | function | `src/jv_parse.c` |

Quality: diversity=0.5, same_kind=1.0, ns_overlap=0.1, unique_files=5

**Pivot:** `jv_copy` (function, c, refs=399)  
File: `src/jv.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.975 | `jv_copy` | function | `src/jv.c` |
| 2 | 0.845 | `jv_object` | function | `src/jv.c` |
| 3 | 0.845 | `jv_object` | function | `src/jv.h` |
| 4 | 0.843 | `jv_unique` | function | `src/jv.h` |
| 5 | 0.840 | `jv_object_merge` | function | `src/jv.h` |
| 6 | 0.838 | `jv_identical` | function | `src/jv.h` |
| 7 | 0.836 | `jv_keys` | function | `src/jv.h` |
| 8 | 0.830 | `jv_get` | function | `src/jv.h` |
| 9 | 0.828 | `jv_true` | function | `src/jv.c` |
| 10 | 0.828 | `jv_true` | function | `src/jv.h` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.1, unique_files=2

### nlohmann-json_a5f86cd4 (cpp)

**Pivot:** `string` (method, cpp, refs=878)  
File: `include/nlohmann/detail/input/json_sax.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.942 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 2 | 0.929 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 3 | 0.900 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 4 | 0.897 | `string` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |
| 5 | 0.897 | `string` | method | `docs/mkdocs/docs/examples/sax_parse__binary.cpp` |
| 6 | 0.887 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 7 | 0.864 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 8 | 0.858 | `boolean` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 9 | 0.850 | `key` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |
| 10 | 0.850 | `key` | method | `docs/mkdocs/docs/examples/sax_parse__binary.cpp` |

Quality: diversity=0.4, same_kind=1.0, ns_overlap=0.4, unique_files=3

**Pivot:** `string` (method, cpp, refs=878)  
File: `include/nlohmann/detail/input/json_sax.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.916 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 2 | 0.900 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 3 | 0.872 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 4 | 0.863 | `boolean` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 5 | 0.856 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 6 | 0.842 | `string` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |
| 7 | 0.842 | `string` | method | `docs/mkdocs/docs/examples/sax_parse__binary.cpp` |
| 8 | 0.838 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 9 | 0.822 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 10 | 0.805 | `number_integer` | method | `include/nlohmann/detail/input/json_sax.hpp` |

Quality: diversity=0.2, same_kind=1.0, ns_overlap=0.4, unique_files=3

**Pivot:** `string` (method, cpp, refs=878)  
File: `include/nlohmann/detail/input/json_sax.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.942 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 2 | 0.930 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 3 | 0.882 | `boolean` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 4 | 0.879 | `key` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 5 | 0.876 | `string` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |
| 6 | 0.876 | `string` | method | `docs/mkdocs/docs/examples/sax_parse__binary.cpp` |
| 7 | 0.872 | `string` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 8 | 0.869 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 9 | 0.834 | `binary` | method | `include/nlohmann/detail/input/json_sax.hpp` |
| 10 | 0.830 | `key` | method | `docs/mkdocs/docs/examples/sax_parse.cpp` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.4, unique_files=3

**Pivot:** `size` (method, cpp, refs=804)  
File: `include/nlohmann/detail/meta/cpp_future.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.839 | `calc_bson_object_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |
| 2 | 0.837 | `std::size_t` | function | `include/nlohmann/detail/string_concat.hpp` |
| 3 | 0.835 | `calc_bson_element_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |
| 4 | 0.834 | `calc_bson_string_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |
| 5 | 0.833 | `calc_bson_integer_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |
| 6 | 0.832 | `size` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 7 | 0.828 | `size_and_type` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 8 | 0.825 | `calc_bson_binary_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |
| 9 | 0.825 | `integer_sequence` | struct | `include/nlohmann/detail/meta/cpp_future.hpp` |
| 10 | 0.824 | `calc_bson_array_size` | method | `include/nlohmann/detail/output/binary_writer.hpp` |

Quality: diversity=0.9, same_kind=0.6, ns_overlap=0.1, unique_files=4

**Pivot:** `size` (function, cpp, refs=788)  
File: `include/nlohmann/json.hpp`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.932 | `max_size` | function | `include/nlohmann/json.hpp` |
| 2 | 0.834 | `count` | function | `include/nlohmann/ordered_map.hpp` |
| 3 | 0.814 | `count` | function | `include/nlohmann/json.hpp` |
| 4 | 0.798 | `size` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |
| 5 | 0.792 | `at` | function | `include/nlohmann/json.hpp` |
| 6 | 0.780 | `end_pos` | function | `include/nlohmann/json.hpp` |
| 7 | 0.779 | `concat_length` | function | `include/nlohmann/detail/string_concat.hpp` |
| 8 | 0.771 | `get_number_unsigned` | function | `include/nlohmann/detail/input/lexer.hpp` |
| 9 | 0.770 | `get_number_integer` | function | `include/nlohmann/detail/input/lexer.hpp` |
| 10 | 0.770 | `size_and_type` | variable | `include/nlohmann/detail/input/binary_reader.hpp` |

Quality: diversity=0.6, same_kind=0.8, ns_overlap=0.1, unique_files=5

### newtonsoft_json_afe705a1 (csharp)

**Pivot:** `Value` (method, csharp, refs=1537)  
File: `Src/Newtonsoft.Json/Linq/Extensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.954 | `Value` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 2 | 0.838 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 3 | 0.825 | `GetTokenValue` | method | `...tonsoft.Json/Serialization/JsonFormatterConverter.cs` |
| 4 | 0.825 | `Convert` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 5 | 0.821 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 6 | 0.821 | `implicit operator JToken` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 7 | 0.819 | `implicit operator JToken` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 8 | 0.816 | `implicit operator JToken` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 9 | 0.816 | `implicit operator JToken` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 10 | 0.814 | `implicit operator JToken` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=0.1, unique_files=3

**Pivot:** `Value` (method, csharp, refs=1537)  
File: `Src/Newtonsoft.Json/Linq/Extensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.954 | `Value` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 2 | 0.853 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 3 | 0.839 | `Convert` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 4 | 0.830 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 5 | 0.827 | `GetTokenValue` | method | `...tonsoft.Json/Serialization/JsonFormatterConverter.cs` |
| 6 | 0.825 | `Convert` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 7 | 0.810 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 8 | 0.804 | `Value` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 9 | 0.800 | `implicit operator JToken` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |
| 10 | 0.798 | `implicit operator JToken` | method | `Src/Newtonsoft.Json/Linq/JToken.cs` |

Quality: diversity=0.4, same_kind=1.0, ns_overlap=0.2, unique_files=3

**Pivot:** `Value` (method, csharp, refs=1432)  
File: `Src/Newtonsoft.Json/Linq/JToken.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.804 | `Value` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |
| 2 | 0.784 | `Convert` | method | `Src/Newtonsoft.Json/Serialization/FormatterConverter.cs` |
| 3 | 0.784 | `Convert` | method | `...tonsoft.Json/Serialization/JsonFormatterConverter.cs` |
| 4 | 0.761 | `TryGetValue` | method | `Src/Newtonsoft.Json/Utilities/DictionaryWrapper.cs` |
| 5 | 0.759 | `Convert` | method | `Src/Newtonsoft.Json/Serialization/FormatterConverter.cs` |
| 6 | 0.759 | `Convert` | method | `...tonsoft.Json/Serialization/JsonFormatterConverter.cs` |
| 7 | 0.754 | `GetDictionaryKey` | method | `Src/Newtonsoft.Json/Serialization/NamingStrategy.cs` |
| 8 | 0.753 | `GetTokenValue` | method | `...tonsoft.Json/Serialization/JsonFormatterConverter.cs` |
| 9 | 0.738 | `Get` | method | `Src/Newtonsoft.Json/Utilities/ThreadSafeStore.cs` |
| 10 | 0.729 | `Values` | method | `Src/Newtonsoft.Json/Linq/Extensions.cs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.1, unique_files=6

**Pivot:** `Read` (method, csharp, refs=1346)  
File: `Src/Newtonsoft.Json/Schema/JsonSchemaBuilder.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.887 | `Read` | method | `Src/Newtonsoft.Json/Schema/JsonSchema.cs` |
| 2 | 0.875 | `Read` | method | `Src/Newtonsoft.Json/Schema/JsonSchema.cs` |
| 3 | 0.837 | `TraceJsonReader` | class | `Src/Newtonsoft.Json/Serialization/TraceJsonReader.cs` |
| 4 | 0.804 | `JsonSchemaBuilder` | class | `Src/Newtonsoft.Json/Schema/JsonSchemaBuilder.cs` |
| 5 | 0.793 | `JsonSerializerInternalReader` | class | `...t.Json/Serialization/JsonSerializerInternalReader.cs` |
| 6 | 0.791 | `MapType` | method | `Src/Newtonsoft.Json/Schema/JsonSchemaBuilder.cs` |
| 7 | 0.789 | `Combine` | method | `Src/Newtonsoft.Json/Schema/JsonSchemaNode.cs` |
| 8 | 0.788 | `JsonTextReader` | class | `Src/Newtonsoft.Json/JsonTextReader.Async.cs` |
| 9 | 0.787 | `MapType` | method | `Src/Newtonsoft.Json/Schema/JsonSchemaBuilder.cs` |
| 10 | 0.787 | `JsonTextReader` | class | `Src/Newtonsoft.Json/JsonTextReader.cs` |

Quality: diversity=0.7, same_kind=0.5, ns_overlap=0.2, unique_files=7

**Pivot:** `Read` (method, csharp, refs=1346)  
File: `Src/Newtonsoft.Json/Linq/JTokenReader.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `Read` | method | `Src/Newtonsoft.Json/Bson/BsonReader.cs` |
| 2 | 1.000 | `Read` | method | `Src/Newtonsoft.Json/JsonTextReader.cs` |
| 3 | 1.000 | `Read` | method | `Src/Newtonsoft.Json/JsonValidatingReader.cs` |
| 4 | 0.967 | `Read` | method | `Src/Newtonsoft.Json/JsonReader.cs` |
| 5 | 0.869 | `ReadAsBoolean` | method | `Src/Newtonsoft.Json/JsonReader.cs` |
| 6 | 0.847 | `ReadAsync` | method | `Src/Newtonsoft.Json/JsonTextReader.Async.cs` |
| 7 | 0.847 | `ReadAsString` | method | `Src/Newtonsoft.Json/JsonValidatingReader.cs` |
| 8 | 0.845 | `ReadAsBoolean` | method | `Src/Newtonsoft.Json/JsonTextReader.cs` |
| 9 | 0.845 | `ReadAsBoolean` | method | `Src/Newtonsoft.Json/JsonValidatingReader.cs` |
| 10 | 0.845 | `ReadAsync` | method | `Src/Newtonsoft.Json/JsonReader.Async.cs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.4, unique_files=6

### riverpod_a7fdc041 (dart)

**Pivot:** `container` (method, dart, refs=2583)  
File: `packages/flutter_riverpod/lib/src/core/provider_scope.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.789 | `ProviderContainer` | method | `...es/flutter_riverpod/lib/src/core/provider_scope.dart` |
| 2 | 0.773 | `_assertContainsDependent` | method | `packages/riverpod/lib/src/core/element.dart` |
| 3 | 0.765 | `ProviderContainer` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 4 | 0.760 | `findDeepestTransitiveDependencyProviderContainer` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 5 | 0.754 | `ProviderScope` | class | `...es/flutter_riverpod/lib/src/core/provider_scope.dart` |
| 6 | 0.748 | `QuestionItem` | class | `examples/stackoverflow/lib/question.dart` |
| 7 | 0.745 | `_LocatedProvider` | class | `...verpod_lint/lib/src/lints/provider_dependencies.dart` |
| 8 | 0.743 | `RiverpodWidgetTesterX` | module | `...es/flutter_riverpod/lib/src/core/provider_scope.dart` |
| 9 | 0.737 | `_getParent` | function | `...es/flutter_riverpod/lib/src/core/provider_scope.dart` |
| 10 | 0.737 | `SomeConsumer` | class | `...ation/from_state_notifier/consumers_dont_change.dart` |

Quality: diversity=0.6, same_kind=0.2, ns_overlap=0.0, unique_files=6

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/riverpod/lib/src/core/provider_subscription.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.800 | `read` | function | `...ges/riverpod/lib/src/core/provider_subscription.dart` |
| 2 | 0.775 | `read` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 3 | 0.758 | `read` | function | `packages/flutter_riverpod/lib/src/core/consumer.dart` |
| 4 | 0.739 | `readElement` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 5 | 0.736 | `read` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |
| 6 | 0.729 | `read` | function | `packages/riverpod_sqflite/lib/src/riverpod_sqflite.dart` |
| 7 | 0.727 | `readSelf` | method | `packages/riverpod/lib/src/core/element.dart` |
| 8 | 0.723 | `read` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 9 | 0.721 | `read` | function | `packages/riverpod/lib/src/core/ref.dart` |
| 10 | 0.718 | `get` | function | `packages/riverpod/lib/src/core/mutations.dart` |

Quality: diversity=0.9, same_kind=0.9, ns_overlap=0.7, unique_files=9

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/riverpod/lib/src/core/persist.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.979 | `read` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 2 | 0.920 | `read` | function | `packages/riverpod_sqflite/lib/src/riverpod_sqflite.dart` |
| 3 | 0.785 | `persist` | method | `...iverpod/lib/src/core/provider/notifier_provider.dart` |
| 4 | 0.762 | `delete` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 5 | 0.759 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 6 | 0.748 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 7 | 0.740 | `get` | method | `website/docs/concepts/about_codegen/main.dart` |
| 8 | 0.740 | `get` | method | `website/docs/concepts/about_codegen/raw.dart` |
| 9 | 0.740 | `get` | method | `...-plugin-content-docs/current/about_codegen/main.dart` |
| 10 | 0.740 | `get` | method | `...s-plugin-content-docs/current/about_codegen/raw.dart` |

Quality: diversity=0.6, same_kind=0.5, ns_overlap=0.2, unique_files=7

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/flutter_riverpod/lib/src/core/widget_ref.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.988 | `read` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 2 | 0.981 | `read` | function | `packages/riverpod/lib/src/core/ref.dart` |
| 3 | 0.951 | `read` | function | `packages/flutter_riverpod/lib/src/core/consumer.dart` |
| 4 | 0.875 | `get` | function | `packages/riverpod/lib/src/core/mutations.dart` |
| 5 | 0.861 | `watch` | function | `packages/riverpod/lib/src/core/ref.dart` |
| 6 | 0.851 | `watch` | function | `packages/flutter_riverpod/lib/src/core/consumer.dart` |
| 7 | 0.821 | `_readProviderElement` | function | `packages/riverpod/lib/src/core/provider_container.dart` |
| 8 | 0.818 | `listenManual` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |
| 9 | 0.814 | `watch` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |
| 10 | 0.811 | `listen` | function | `packages/flutter_riverpod/lib/src/core/widget_ref.dart` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.3, unique_files=5

**Pivot:** `read` (function, dart, refs=1910)  
File: `packages/riverpod/lib/src/core/persist.dart`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.979 | `read` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 2 | 0.932 | `read` | function | `packages/riverpod_sqflite/lib/src/riverpod_sqflite.dart` |
| 3 | 0.795 | `persist` | method | `...iverpod/lib/src/core/provider/notifier_provider.dart` |
| 4 | 0.788 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 5 | 0.775 | `write` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 6 | 0.771 | `delete` | function | `packages/riverpod/lib/src/core/persist.dart` |
| 7 | 0.770 | `_callEncode` | method | `...iverpod/lib/src/core/provider/notifier_provider.dart` |
| 8 | 0.768 | `get` | method | `website/docs/concepts/about_codegen/main.dart` |
| 9 | 0.768 | `get` | method | `website/docs/concepts/about_codegen/raw.dart` |
| 10 | 0.768 | `get` | method | `...-plugin-content-docs/current/about_codegen/main.dart` |

Quality: diversity=0.6, same_kind=0.5, ns_overlap=0.2, unique_files=6

### phoenix_ac16deb4 (elixir)

**Pivot:** `inspect` (function, elixir, refs=419)  
File: `lib/phoenix/socket/message.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.799 | `Phoenix.Socket.Message` | module | `lib/phoenix/socket/message.ex` |
| 2 | 0.793 | `phoenix_socket_connected` | function | `lib/phoenix/logger.ex` |
| 3 | 0.792 | `phoenix_socket_drain` | function | `lib/phoenix/logger.ex` |
| 4 | 0.770 | `decode!` | function | `lib/phoenix/socket/serializer.ex` |
| 5 | 0.770 | `t` | type | `lib/phoenix/socket/message.ex` |
| 6 | 0.769 | `update` | function | `lib/phoenix/presence.ex` |
| 7 | 0.766 | `handle_info` | function | `lib/phoenix/channel/server.ex` |
| 8 | 0.764 | `Phoenix.Socket.Message` | struct | `lib/phoenix/socket/message.ex` |
| 9 | 0.763 | `phoenix_error_rendered` | function | `lib/phoenix/logger.ex` |
| 10 | 0.763 | `phoenix_channel_handled_in` | function | `lib/phoenix/logger.ex` |

Quality: diversity=0.7, same_kind=0.7, ns_overlap=0.0, unique_files=5

**Pivot:** `join` (function, elixir, refs=414)  
File: `priv/templates/phx.gen.channel/channel.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.864 | `join` | function | `lib/phoenix/channel/server.ex` |
| 2 | 0.833 | `channel_join` | function | `lib/phoenix/channel/server.ex` |
| 3 | 0.828 | `join` | function | `lib/phoenix/channel.ex` |
| 4 | 0.825 | `init_join` | function | `lib/phoenix/channel/server.ex` |
| 5 | 0.810 | `assert_joined!` | function | `lib/phoenix/channel.ex` |
| 6 | 0.809 | `assert_joined!` | function | `lib/phoenix/channel.ex` |
| 7 | 0.807 | `join` | function | `lib/phoenix/router/scope.ex` |
| 8 | 0.804 | `join_as` | function | `lib/phoenix/router/scope.ex` |
| 9 | 0.803 | `join_as` | function | `lib/phoenix/router/scope.ex` |
| 10 | 0.793 | `join_result` | function | `lib/phoenix/logger.ex` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.3, unique_files=4

**Pivot:** `join` (function, elixir, refs=414)  
File: `lib/phoenix/channel.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.828 | `join` | function | `priv/templates/phx.gen.channel/channel.ex` |
| 2 | 0.824 | `channel_join` | function | `lib/phoenix/channel/server.ex` |
| 3 | 0.819 | `init_join` | function | `lib/phoenix/channel/server.ex` |
| 4 | 0.794 | `join` | function | `lib/phoenix/channel/server.ex` |
| 5 | 0.773 | `handle_join` | function | `lib/phoenix/presence.ex` |
| 6 | 0.759 | `join` | method | `assets/js/phoenix/channel.js` |
| 7 | 0.759 | `join` | method | `priv/static/phoenix.cjs.js` |
| 8 | 0.759 | `join` | method | `priv/static/phoenix.js` |
| 9 | 0.759 | `join` | method | `priv/static/phoenix.mjs` |
| 10 | 0.758 | `connect` | function | `lib/phoenix/socket.ex` |

Quality: diversity=1.0, same_kind=0.6, ns_overlap=0.6, unique_files=8

**Pivot:** `join` (method, javascript, refs=414)  
File: `assets/js/phoenix/channel.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `join` | method | `priv/static/phoenix.cjs.js` |
| 2 | 1.000 | `join` | method | `priv/static/phoenix.js` |
| 3 | 1.000 | `join` | method | `priv/static/phoenix.mjs` |
| 4 | 0.857 | `channel_join` | function | `lib/phoenix/channel/server.ex` |
| 5 | 0.838 | `join` | function | `lib/phoenix/channel/server.ex` |
| 6 | 0.821 | `init_join` | function | `lib/phoenix/channel/server.ex` |
| 7 | 0.795 | `phoenix_channel_joined` | function | `lib/phoenix/logger.ex` |
| 8 | 0.794 | `leave` | method | `assets/js/phoenix/channel.js` |
| 9 | 0.794 | `leave` | method | `priv/static/phoenix.cjs.js` |
| 10 | 0.794 | `leave` | method | `priv/static/phoenix.js` |

Quality: diversity=0.9, same_kind=0.6, ns_overlap=0.4, unique_files=6

**Pivot:** `join` (function, elixir, refs=413)  
File: `lib/phoenix/channel/server.ex`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.906 | `channel_join` | function | `lib/phoenix/channel/server.ex` |
| 2 | 0.884 | `init_join` | function | `lib/phoenix/channel/server.ex` |
| 3 | 0.864 | `join` | function | `priv/templates/phx.gen.channel/channel.ex` |
| 4 | 0.838 | `join` | method | `assets/js/phoenix/channel.js` |
| 5 | 0.838 | `join` | method | `priv/static/phoenix.cjs.js` |
| 6 | 0.838 | `join` | method | `priv/static/phoenix.js` |
| 7 | 0.838 | `join` | method | `priv/static/phoenix.mjs` |
| 8 | 0.820 | `phoenix_channel_joined` | function | `lib/phoenix/logger.ex` |
| 9 | 0.820 | `assert_joined!` | function | `lib/phoenix/channel.ex` |
| 10 | 0.818 | `assert_joined!` | function | `lib/phoenix/channel.ex` |

Quality: diversity=0.8, same_kind=0.6, ns_overlap=0.5, unique_files=8

### cobra_8b201fd3 (go)

**Pivot:** `Flags` (method, go, refs=170)  
File: `command.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.837 | `Flag` | method | `command.go` |
| 2 | 0.822 | `DebugFlags` | method | `command.go` |
| 3 | 0.820 | `LocalFlags` | method | `command.go` |
| 4 | 0.802 | `NonInheritedFlags` | method | `command.go` |
| 5 | 0.791 | `MarkFlagsRequiredTogether` | method | `flag_groups.go` |
| 6 | 0.788 | `FlagErrorFunc` | method | `command.go` |
| 7 | 0.788 | `ArgsLenAtDash` | method | `command.go` |
| 8 | 0.787 | `PersistentFlags` | method | `command.go` |
| 9 | 0.782 | `InheritedFlags` | method | `command.go` |
| 10 | 0.780 | `MarkFlagsMutuallyExclusive` | method | `flag_groups.go` |

Quality: diversity=0.2, same_kind=1.0, ns_overlap=0.0, unique_files=2

**Pivot:** `AddCommand` (method, go, refs=156)  
File: `command.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.878 | `AddGroup` | method | `command.go` |
| 2 | 0.828 | `Parent` | method | `command.go` |
| 3 | 0.822 | `RemoveCommand` | method | `command.go` |
| 4 | 0.783 | `Commands` | method | `command.go` |
| 5 | 0.776 | `IsAdditionalHelpTopicCommand` | method | `command.go` |
| 6 | 0.774 | `ResetCommands` | method | `command.go` |
| 7 | 0.770 | `SetHelpCommand` | method | `command.go` |
| 8 | 0.769 | `writeCommands` | function | `bash_completions.go` |
| 9 | 0.765 | `updateParentsPflags` | method | `command.go` |
| 10 | 0.761 | `checkCommandGroups` | method | `command.go` |

Quality: diversity=0.1, same_kind=0.9, ns_overlap=0.0, unique_files=2

**Pivot:** `Name` (method, go, refs=116)  
File: `command.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.812 | `UsageString` | method | `command.go` |
| 2 | 0.793 | `DisplayName` | method | `command.go` |
| 3 | 0.793 | `CalledAs` | method | `command.go` |
| 4 | 0.791 | `Flag` | method | `command.go` |
| 5 | 0.777 | `UseLine` | method | `command.go` |
| 6 | 0.772 | `NamePadding` | method | `command.go` |
| 7 | 0.770 | `UsageTemplate` | method | `command.go` |
| 8 | 0.763 | `Usage` | method | `command.go` |
| 9 | 0.744 | `NameAndAliases` | method | `command.go` |
| 10 | 0.740 | `Context` | method | `command.go` |

Quality: diversity=0.0, same_kind=1.0, ns_overlap=0.0, unique_files=1

**Pivot:** `Error` (method, go, refs=77)  
File: `completions.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.826 | `execute` | method | `command.go` |
| 2 | 0.819 | `FlagErrorFunc` | method | `command.go` |
| 3 | 0.778 | `SetFlagErrorFunc` | method | `command.go` |
| 4 | 0.777 | `ValidateArgs` | method | `command.go` |
| 5 | 0.772 | `ErrOrStderr` | method | `command.go` |
| 6 | 0.772 | `flagCompError` | class | `completions.go` |
| 7 | 0.755 | `CompError` | function | `completions.go` |
| 8 | 0.748 | `Execute` | method | `command.go` |
| 9 | 0.744 | `ValidateRequiredFlags` | method | `command.go` |
| 10 | 0.738 | `Help` | method | `command.go` |

Quality: diversity=0.8, same_kind=0.8, ns_overlap=0.0, unique_files=2

**Pivot:** `WriteStringAndCheck` (function, go, refs=61)  
File: `cobra.go`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.710 | `writeLocalNonPersistentFlag` | function | `bash_completions.go` |
| 2 | 0.707 | `writePostscript` | function | `bash_completions.go` |
| 3 | 0.698 | `writePreamble` | function | `bash_completions.go` |
| 4 | 0.692 | `writeShortFlag` | function | `bash_completions.go` |
| 5 | 0.679 | `writeRequiredFlag` | function | `bash_completions.go` |
| 6 | 0.679 | `indentString` | function | `doc/rest_docs.go` |
| 7 | 0.679 | `writeFlag` | function | `bash_completions.go` |
| 8 | 0.675 | `writeFlagHandler` | function | `bash_completions.go` |
| 9 | 0.672 | `ErrOrStderr` | method | `command.go` |
| 10 | 0.671 | `writeRequiredNouns` | function | `bash_completions.go` |

Quality: diversity=1.0, same_kind=0.9, ns_overlap=0.0, unique_files=3

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
| 9 | 0.991 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 10 | 0.953 | `toImmutableSortedMap` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |

Quality: diversity=0.0, same_kind=1.0, ns_overlap=0.9, unique_files=1

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
| 9 | 0.991 | `of` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |
| 10 | 0.953 | `toImmutableSortedMap` | method | `...er/com/google/common/collect/ImmutableSortedMap.java` |

Quality: diversity=0.0, same_kind=1.0, ns_overlap=0.9, unique_files=1

**Pivot:** `of` (method, java, refs=6412)  
File: `guava/src/com/google/common/primitives/ImmutableIntArray.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 2 | 0.990 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 3 | 0.990 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 4 | 0.987 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 5 | 0.987 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 6 | 0.982 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 7 | 0.982 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 8 | 0.974 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 9 | 0.974 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |
| 10 | 0.942 | `of` | method | `.../com/google/common/primitives/ImmutableIntArray.java` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `of` (method, java, refs=6412)  
File: `guava/src/com/google/common/base/Optional.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `android/guava/src/com/google/common/base/Optional.java` |
| 2 | 0.885 | `or` | method | `android/guava/src/com/google/common/base/Present.java` |
| 3 | 0.885 | `or` | method | `guava/src/com/google/common/base/Present.java` |
| 4 | 0.879 | `or` | method | `android/guava/src/com/google/common/base/Present.java` |
| 5 | 0.879 | `or` | method | `guava/src/com/google/common/base/Present.java` |
| 6 | 0.870 | `or` | method | `android/guava/src/com/google/common/base/Absent.java` |
| 7 | 0.870 | `or` | method | `guava/src/com/google/common/base/Absent.java` |
| 8 | 0.864 | `or` | method | `android/guava/src/com/google/common/base/Absent.java` |
| 9 | 0.864 | `or` | method | `guava/src/com/google/common/base/Absent.java` |
| 10 | 0.858 | `or` | method | `android/guava/src/com/google/common/base/Present.java` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.1, unique_files=5

**Pivot:** `of` (method, java, refs=6412)  
File: `guava/src/com/google/common/primitives/ImmutableLongArray.java`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 2 | 0.990 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 3 | 0.990 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 4 | 0.974 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 5 | 0.974 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 6 | 0.971 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 7 | 0.971 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 8 | 0.963 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 9 | 0.963 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |
| 10 | 0.961 | `of` | method | `...com/google/common/primitives/ImmutableLongArray.java` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=1.0, unique_files=2

### express_8cefd559 (javascript)

**Pivot:** `get` (method, javascript, refs=1024)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.904 | `get` | function | `lib/application.js` |
| 2 | 0.821 | `get` | method | `examples/route-map/index.js` |
| 3 | 0.779 | `text` | method | `examples/content-negotiation/index.js` |
| 4 | 0.779 | `text` | method | `lib/response.js` |
| 5 | 0.764 | `get` | function | `examples/route-map/index.js` |
| 6 | 0.760 | `html` | method | `examples/content-negotiation/index.js` |
| 7 | 0.760 | `html` | method | `examples/error-pages/index.js` |
| 8 | 0.760 | `html` | method | `lib/response.js` |
| 9 | 0.759 | `json` | method | `examples/content-negotiation/index.js` |
| 10 | 0.759 | `json` | method | `examples/error-pages/index.js` |

Quality: diversity=0.9, same_kind=0.8, ns_overlap=0.3, unique_files=5

**Pivot:** `get` (function, javascript, refs=1024)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.904 | `get` | method | `lib/application.js` |
| 2 | 0.869 | `get` | function | `examples/route-map/index.js` |
| 3 | 0.810 | `text` | function | `examples/content-negotiation/index.js` |
| 4 | 0.810 | `text` | function | `lib/response.js` |
| 5 | 0.790 | `get` | method | `examples/route-map/index.js` |
| 6 | 0.785 | `html` | function | `examples/content-negotiation/index.js` |
| 7 | 0.785 | `html` | function | `examples/error-pages/index.js` |
| 8 | 0.785 | `html` | function | `lib/response.js` |
| 9 | 0.779 | `json` | function | `examples/content-negotiation/index.js` |
| 10 | 0.779 | `json` | function | `examples/error-pages/index.js` |

Quality: diversity=0.9, same_kind=0.8, ns_overlap=0.3, unique_files=5

**Pivot:** `get` (method, javascript, refs=1012)  
File: `lib/response.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.983 | `res.get` | function | `lib/response.js` |
| 2 | 0.866 | `res.header` | function | `lib/response.js` |
| 3 | 0.863 | `header` | method | `lib/response.js` |
| 4 | 0.834 | `get` | function | `examples/route-map/index.js` |
| 5 | 0.823 | `get` | method | `examples/route-map/index.js` |
| 6 | 0.812 | `res.append` | function | `lib/response.js` |
| 7 | 0.796 | `append` | method | `lib/response.js` |
| 8 | 0.784 | `header` | method | `lib/request.js` |
| 9 | 0.776 | `req.header` | function | `lib/request.js` |
| 10 | 0.776 | `headers` | function | `lib/response.js` |

Quality: diversity=0.4, same_kind=0.4, ns_overlap=0.3, unique_files=3

**Pivot:** `use` (method, javascript, refs=641)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.987 | `app.use` | function | `lib/application.js` |
| 2 | 0.813 | `app.route` | function | `lib/application.js` |
| 3 | 0.809 | `route` | method | `lib/application.js` |
| 4 | 0.796 | `app.param` | function | `lib/application.js` |
| 5 | 0.795 | `param` | method | `lib/application.js` |
| 6 | 0.732 | `app[method]` | function | `lib/application.js` |
| 7 | 0.729 | `app.map` | function | `examples/route-map/index.js` |
| 8 | 0.710 | `app` | function | `lib/express.js` |
| 9 | 0.710 | `map` | method | `examples/route-map/index.js` |
| 10 | 0.695 | `all` | method | `lib/application.js` |

Quality: diversity=0.3, same_kind=0.4, ns_overlap=0.1, unique_files=3

**Pivot:** `set` (method, javascript, refs=593)  
File: `lib/application.js`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.979 | `app.set` | function | `lib/application.js` |
| 2 | 0.785 | `enable` | method | `lib/application.js` |
| 3 | 0.783 | `app.enable` | function | `lib/application.js` |
| 4 | 0.759 | `app.disable` | function | `lib/application.js` |
| 5 | 0.744 | `disabled` | method | `lib/application.js` |
| 6 | 0.744 | `app.disabled` | function | `lib/application.js` |
| 7 | 0.742 | `disable` | method | `lib/application.js` |
| 8 | 0.735 | `enabled` | method | `lib/application.js` |
| 9 | 0.729 | `app.enabled` | function | `lib/application.js` |
| 10 | 0.721 | `res.header` | function | `lib/response.js` |

Quality: diversity=0.1, same_kind=0.4, ns_overlap=0.1, unique_files=2

### moshi_c9c5a600 (kotlin)

**Pivot:** `fromJson` (method, kotlin, refs=435)  
File: `moshi-adapters/src/main/java/com/squareup/moshi/adapters/EnumJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.939 | `toJson` | method | `.../java/com/squareup/moshi/adapters/EnumJsonAdapter.kt` |
| 2 | 0.885 | `fromJson` | method | `...om/squareup/moshi/adapters/Rfc3339DateJsonAdapter.kt` |
| 3 | 0.869 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/Moshi.kt` |
| 4 | 0.866 | `EnumJsonAdapter` | class | `.../java/com/squareup/moshi/adapters/EnumJsonAdapter.kt` |
| 5 | 0.864 | `toString` | method | `.../java/com/squareup/moshi/adapters/EnumJsonAdapter.kt` |
| 6 | 0.864 | `fromJson` | method | `...eup/moshi/kotlin/reflect/KotlinJsonAdapterFactory.kt` |
| 7 | 0.864 | `fromJson` | method | `...c/main/java/com/squareup/moshi/recipes/JsonString.kt` |
| 8 | 0.861 | `FallbackEnumJsonAdapter` | class | `...in/java/com/squareup/moshi/recipes/FallbackEnum.java` |
| 9 | 0.859 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 10 | 0.858 | `fromJson` | method | `...src/main/java/com/squareup/moshi/recipes/Unwrap.java` |

Quality: diversity=0.7, same_kind=0.8, ns_overlap=0.6, unique_files=8

**Pivot:** `fromJson` (method, kotlin, refs=435)  
File: `moshi/src/main/java/com/squareup/moshi/internal/RecordJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.921 | `fromJson` | method | `...com/squareup/moshi/internal/AdapterMethodsFactory.kt` |
| 2 | 0.920 | `toJson` | method | `...ava/com/squareup/moshi/internal/RecordJsonAdapter.kt` |
| 3 | 0.912 | `fromJson` | method | `...c/main/java/com/squareup/moshi/recipes/JsonString.kt` |
| 4 | 0.910 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 5 | 0.909 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 6 | 0.909 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 7 | 0.909 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 8 | 0.909 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 9 | 0.909 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |
| 10 | 0.909 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.9, unique_files=6

**Pivot:** `fromJson` (method, kotlin, refs=435)  
File: `moshi-adapters/src/main/java/com/squareup/moshi/adapters/Rfc3339DateJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.954 | `fromJson` | method | `...in/java/com/squareup/moshi/Rfc3339DateJsonAdapter.kt` |
| 2 | 0.889 | `toJson` | method | `...om/squareup/moshi/adapters/Rfc3339DateJsonAdapter.kt` |
| 3 | 0.885 | `fromJson` | method | `.../java/com/squareup/moshi/adapters/EnumJsonAdapter.kt` |
| 4 | 0.881 | `fromJson` | method | `...src/main/java/com/squareup/moshi/recipes/Unwrap.java` |
| 5 | 0.880 | `fromJson` | method | `...reup/moshi/recipes/DefaultOnDataMismatchAdapter.java` |
| 6 | 0.880 | `fromJson` | method | `...in/java/com/squareup/moshi/recipes/FallbackEnum.java` |
| 7 | 0.880 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 8 | 0.877 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/Moshi.kt` |
| 9 | 0.876 | `fromJson` | method | `...c/main/java/com/squareup/moshi/recipes/JsonString.kt` |
| 10 | 0.868 | `toJson` | method | `...in/java/com/squareup/moshi/Rfc3339DateJsonAdapter.kt` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.8, unique_files=9

**Pivot:** `fromJson` (method, kotlin, refs=434)  
File: `moshi/src/main/java/com/squareup/moshi/internal/ArrayJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.936 | `fromJson` | method | `...com/squareup/moshi/internal/CollectionJsonAdapter.kt` |
| 2 | 0.926 | `fromJson` | method | `...c/main/java/com/squareup/moshi/recipes/JsonString.kt` |
| 3 | 0.917 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 4 | 0.915 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |
| 5 | 0.915 | `fromJson` | method | `...reup/moshi/adapters/PolymorphicJsonAdapterFactory.kt` |
| 6 | 0.915 | `fromJson` | method | `...com/squareup/moshi/internal/AdapterMethodsFactory.kt` |
| 7 | 0.909 | `fromJson` | method | `...va/com/squareup/moshi/internal/NonNullJsonAdapter.kt` |
| 8 | 0.908 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 9 | 0.908 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 10 | 0.908 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=7

**Pivot:** `fromJson` (method, kotlin, refs=434)  
File: `moshi/src/main/java/com/squareup/moshi/internal/CollectionJsonAdapter.kt`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.936 | `fromJson` | method | `...java/com/squareup/moshi/internal/ArrayJsonAdapter.kt` |
| 2 | 0.894 | `toJson` | method | `...com/squareup/moshi/internal/CollectionJsonAdapter.kt` |
| 3 | 0.876 | `fromJson` | method | `...a16/com/squareup/moshi/internal/RecordJsonAdapter.kt` |
| 4 | 0.874 | `fromJson` | method | `...c/main/java/com/squareup/moshi/recipes/JsonString.kt` |
| 5 | 0.874 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/Moshi.kt` |
| 6 | 0.873 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 7 | 0.868 | `fromJson` | method | `moshi/src/main/java/com/squareup/moshi/JsonAdapter.kt` |
| 8 | 0.868 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 9 | 0.865 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |
| 10 | 0.864 | `fromJson` | method | `.../com/squareup/moshi/internal/StandardJsonAdapters.kt` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.9, unique_files=7

### lite_f7e95a20 (lua)

**Pivot:** `GLenum` (type, c, refs=1400)  
File: `winlib/SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLenum` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.915 | `GLenum` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.915 | `GLenum` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.865 | `GLboolean` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 5 | 0.865 | `GLboolean` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 6 | 0.863 | `GLclampf` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.863 | `GLclampf` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.861 | `GLfixed` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 9 | 0.861 | `GLfixed` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 10 | 0.860 | `GLchar` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=0.3, unique_files=4

**Pivot:** `GLenum` (type, c, refs=1400)  
File: `winlib/SDL2-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLenum` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.915 | `GLenum` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.915 | `GLenum` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.865 | `GLboolean` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 5 | 0.865 | `GLboolean` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 6 | 0.863 | `GLclampf` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.863 | `GLclampf` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.861 | `GLfixed` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 9 | 0.861 | `GLfixed` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 10 | 0.860 | `GLchar` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.3, unique_files=4

**Pivot:** `GLint` (type, c, refs=1103)  
File: `winlib/SDL2-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLint` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.915 | `GLint` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.915 | `GLint` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.888 | `GLint64` | type | `...i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 5 | 0.888 | `GLint64` | type | `...6_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 6 | 0.881 | `GLint` | variable | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.860 | `GLfloat` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.860 | `GLfloat` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 9 | 0.858 | `GLbyte` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 10 | 0.858 | `GLbyte` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.8, same_kind=0.9, ns_overlap=0.4, unique_files=6

**Pivot:** `GLint` (type, c, refs=1103)  
File: `winlib/SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLint` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 2 | 0.915 | `GLint` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 3 | 0.915 | `GLint` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 4 | 0.888 | `GLint64` | type | `...i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 5 | 0.888 | `GLint64` | type | `...6_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2ext.h` |
| 6 | 0.881 | `GLint` | variable | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 7 | 0.860 | `GLfloat` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 8 | 0.860 | `GLfloat` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 9 | 0.858 | `GLbyte` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 10 | 0.858 | `GLbyte` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |

Quality: diversity=0.7, same_kind=0.9, ns_overlap=0.4, unique_files=6

**Pivot:** `GLenum` (type, c, refs=1093)  
File: `winlib/SDL2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `GLenum` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 2 | 0.915 | `GLenum` | type | `...10/i686-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 3 | 0.915 | `GLenum` | type | `.../x86_64-w64-mingw32/include/SDL2/SDL_opengles2_gl2.h` |
| 4 | 0.866 | `s` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 5 | 0.866 | `s` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 6 | 0.866 | `s` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 7 | 0.866 | `s` | type | `...-2.0.10/x86_64-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 8 | 0.866 | `s` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 9 | 0.866 | `s` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |
| 10 | 0.866 | `s` | type | `...L2-2.0.10/i686-w64-mingw32/include/SDL2/SDL_opengl.h` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.3, unique_files=4

### slim_dce0015d (php)

**Pivot:** `handle` (method, php, refs=112)  
File: `Slim/Routing/RouteRunner.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.947 | `handle` | method | `Slim/App.php` |
| 2 | 0.946 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.946 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 4 | 0.946 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 5 | 0.942 | `handle` | method | `Slim/Routing/Route.php` |
| 6 | 0.901 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 7 | 0.887 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 8 | 0.867 | `run` | method | `Slim/Routing/Route.php` |
| 9 | 0.861 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |
| 10 | 0.861 | `process` | method | `Slim/Middleware/ContentLengthMiddleware.php` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.6, unique_files=6

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/MiddlewareDispatcher.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 2 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.994 | `handle` | method | `Slim/App.php` |
| 4 | 0.989 | `handle` | method | `Slim/Routing/Route.php` |
| 5 | 0.946 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 6 | 0.934 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 7 | 0.911 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 8 | 0.908 | `run` | method | `Slim/Routing/Route.php` |
| 9 | 0.880 | `run` | method | `Slim/Interfaces/RouteInterface.php` |
| 10 | 0.872 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.6, unique_files=7

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/MiddlewareDispatcher.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 2 | 1.000 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.994 | `handle` | method | `Slim/App.php` |
| 4 | 0.989 | `handle` | method | `Slim/Routing/Route.php` |
| 5 | 0.946 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 6 | 0.934 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 7 | 0.911 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 8 | 0.908 | `run` | method | `Slim/Routing/Route.php` |
| 9 | 0.880 | `run` | method | `Slim/Interfaces/RouteInterface.php` |
| 10 | 0.872 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.6, unique_files=7

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/MiddlewareDispatcher.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.934 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 2 | 0.934 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.934 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 4 | 0.932 | `handle` | method | `Slim/App.php` |
| 5 | 0.926 | `handle` | method | `Slim/Routing/Route.php` |
| 6 | 0.901 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 7 | 0.873 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 8 | 0.859 | `run` | method | `Slim/Routing/Route.php` |
| 9 | 0.845 | `run` | method | `Slim/Interfaces/RouteInterface.php` |
| 10 | 0.827 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=0.6, unique_files=7

**Pivot:** `handle` (method, php, refs=111)  
File: `Slim/Routing/Route.php`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.989 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 2 | 0.989 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 3 | 0.989 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 4 | 0.985 | `handle` | method | `Slim/App.php` |
| 5 | 0.942 | `handle` | method | `Slim/Routing/RouteRunner.php` |
| 6 | 0.926 | `handle` | method | `Slim/MiddlewareDispatcher.php` |
| 7 | 0.921 | `run` | method | `Slim/Routing/Route.php` |
| 8 | 0.915 | `handleException` | method | `Slim/Middleware/ErrorMiddleware.php` |
| 9 | 0.874 | `run` | method | `Slim/Interfaces/RouteInterface.php` |
| 10 | 0.868 | `process` | method | `Slim/Middleware/BodyParsingMiddleware.php` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.6, unique_files=7

### flask_9045020a (python)

**Pivot:** `get` (method, python, refs=386)  
File: `src/flask/sansio/scaffold.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.871 | `put` | method | `src/flask/sansio/scaffold.py` |
| 2 | 0.846 | `_method_route` | method | `src/flask/sansio/scaffold.py` |
| 3 | 0.829 | `post` | method | `src/flask/sansio/scaffold.py` |
| 4 | 0.828 | `patch` | method | `src/flask/sansio/scaffold.py` |
| 5 | 0.828 | `route` | method | `src/flask/sansio/scaffold.py` |
| 6 | 0.786 | `decorator` | method | `src/flask/sansio/scaffold.py` |
| 7 | 0.783 | `delete` | method | `src/flask/sansio/scaffold.py` |
| 8 | 0.748 | `add_url_rule` | method | `src/flask/sansio/scaffold.py` |
| 9 | 0.740 | `add_url_rule` | method | `src/flask/sansio/app.py` |
| 10 | 0.735 | `decorator` | method | `src/flask/sansio/blueprints.py` |

Quality: diversity=0.2, same_kind=1.0, ns_overlap=0.0, unique_files=3

**Pivot:** `get` (method, python, refs=379)  
File: `src/flask/ctx.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.872 | `__getattr__` | function | `src/flask/ctx.py` |
| 2 | 0.872 | `__getattr__` | function | `src/flask/globals.py` |
| 3 | 0.856 | `setdefault` | method | `src/flask/ctx.py` |
| 4 | 0.837 | `__getattr__` | method | `src/flask/ctx.py` |
| 5 | 0.800 | `__setattr__` | method | `src/flask/ctx.py` |
| 6 | 0.777 | `pop` | method | `src/flask/ctx.py` |
| 7 | 0.776 | `attr` | variable | `src/flask/cli.py` |
| 8 | 0.769 | `self_name` | variable | `src/flask/sansio/blueprints.py` |
| 9 | 0.750 | `__get__` | method | `src/flask/config.py` |
| 10 | 0.741 | `__get__` | method | `src/flask/config.py` |

Quality: diversity=0.5, same_kind=0.6, ns_overlap=0.0, unique_files=5

**Pivot:** `route` (method, python, refs=288)  
File: `src/flask/sansio/scaffold.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.854 | `decorator` | method | `src/flask/sansio/scaffold.py` |
| 2 | 0.852 | `_method_route` | method | `src/flask/sansio/scaffold.py` |
| 3 | 0.841 | `add_url_rule` | method | `src/flask/sansio/blueprints.py` |
| 4 | 0.838 | `add_url_rule` | method | `src/flask/sansio/scaffold.py` |
| 5 | 0.837 | `put` | method | `src/flask/sansio/scaffold.py` |
| 6 | 0.828 | `get` | method | `src/flask/sansio/scaffold.py` |
| 7 | 0.818 | `add_url_rule` | method | `src/flask/sansio/app.py` |
| 8 | 0.816 | `add_url_rule` | method | `src/flask/sansio/blueprints.py` |
| 9 | 0.815 | `post` | method | `src/flask/sansio/scaffold.py` |
| 10 | 0.814 | `endpoint` | method | `src/flask/sansio/scaffold.py` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.0, unique_files=3

**Pivot:** `Flask` (class, python, refs=125)  
File: `src/flask/app.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.901 | `App` | class | `src/flask/sansio/app.py` |
| 2 | 0.799 | `FlaskClient` | class | `src/flask/testing.py` |
| 3 | 0.778 | `Request` | class | `src/flask/wrappers.py` |
| 4 | 0.773 | `__call__` | method | `src/flask/app.py` |
| 5 | 0.772 | `FlaskCliRunner` | class | `src/flask/testing.py` |
| 6 | 0.769 | `FlaskProxy` | class | `src/flask/globals.py` |
| 7 | 0.763 | `Scaffold` | class | `src/flask/sansio/scaffold.py` |
| 8 | 0.753 | `FlaskTask` | class | `examples/celery/src/task_app/__init__.py` |
| 9 | 0.748 | `FlaskGroup` | class | `src/flask/cli.py` |
| 10 | 0.743 | `wrapper` | function | `src/flask/app.py` |

Quality: diversity=0.8, same_kind=0.8, ns_overlap=0.0, unique_files=8

**Pivot:** `jinja_env` (method, python, refs=124)  
File: `src/flask/sansio/app.py`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.859 | `jinja_loader` | method | `src/flask/sansio/scaffold.py` |
| 2 | 0.819 | `create_jinja_environment` | method | `src/flask/sansio/app.py` |
| 3 | 0.792 | `Environment` | class | `src/flask/templating.py` |
| 4 | 0.778 | `create_jinja_environment` | method | `src/flask/app.py` |
| 5 | 0.750 | `App` | class | `src/flask/sansio/app.py` |
| 6 | 0.743 | `from_prefixed_env` | method | `src/flask/config.py` |
| 7 | 0.738 | `get_source` | method | `src/flask/templating.py` |
| 8 | 0.730 | `create_global_jinja_loader` | method | `src/flask/sansio/app.py` |
| 9 | 0.716 | `from_envvar` | method | `src/flask/config.py` |
| 10 | 0.711 | `app_template_filter` | method | `src/flask/sansio/blueprints.py` |

Quality: diversity=0.7, same_kind=0.8, ns_overlap=0.0, unique_files=6

### sinatra_86eed2fe (ruby)

**Pivot:** `get` (method, ruby, refs=1210)  
File: `sinatra-contrib/lib/sinatra/multi_route.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.931 | `route` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 2 | 0.905 | `route_args` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 3 | 0.904 | `options` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 4 | 0.899 | `head` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 5 | 0.892 | `put` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 6 | 0.879 | `post` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 7 | 0.871 | `patch` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 8 | 0.869 | `MultiRoute` | module | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 9 | 0.824 | `delete` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 10 | 0.796 | `new` | method | `sinatra-contrib/lib/sinatra/extension.rb` |

Quality: diversity=0.1, same_kind=0.9, ns_overlap=0.0, unique_files=2

**Pivot:** `get` (method, ruby, refs=1209)  
File: `lib/sinatra/base.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.842 | `options` | method | `lib/sinatra/base.rb` |
| 2 | 0.827 | `link` | method | `lib/sinatra/base.rb` |
| 3 | 0.823 | `head` | method | `lib/sinatra/base.rb` |
| 4 | 0.813 | `put` | method | `lib/sinatra/base.rb` |
| 5 | 0.808 | `get` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 6 | 0.801 | `route` | method | `lib/sinatra/base.rb` |
| 7 | 0.794 | `post` | method | `lib/sinatra/base.rb` |
| 8 | 0.793 | `get` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 9 | 0.787 | `patch` | method | `lib/sinatra/base.rb` |
| 10 | 0.780 | `fetch` | method | `sinatra-contrib/lib/sinatra/cookies.rb` |

Quality: diversity=0.3, same_kind=1.0, ns_overlap=0.2, unique_files=4

**Pivot:** `get` (method, ruby, refs=1209)  
File: `sinatra-contrib/lib/sinatra/runner.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.968 | `get_url` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 2 | 0.893 | `get_response` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 3 | 0.837 | `get_stream` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 4 | 0.833 | `get_https_url` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 5 | 0.808 | `get` | method | `lib/sinatra/base.rb` |
| 6 | 0.762 | `link` | method | `sinatra-contrib/lib/sinatra/link_header.rb` |
| 7 | 0.749 | `get` | method | `sinatra-contrib/lib/sinatra/multi_route.rb` |
| 8 | 0.743 | `link?` | method | `lib/sinatra/base.rb` |
| 9 | 0.741 | `inspect` | method | `lib/sinatra/base.rb` |
| 10 | 0.740 | `fetch` | method | `lib/sinatra/indifferent_hash.rb` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=0.2, unique_files=5

**Pivot:** `to` (method, ruby, refs=680)  
File: `lib/sinatra/base.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.964 | `url` | method | `lib/sinatra/base.rb` |
| 2 | 0.726 | `status` | method | `lib/sinatra/base.rb` |
| 3 | 0.725 | `with_params` | method | `lib/sinatra/base.rb` |
| 4 | 0.715 | `get_response` | method | `sinatra-contrib/lib/sinatra/runner.rb` |
| 5 | 0.704 | `escape_url` | method | `rack-protection/lib/rack/protection/escaped_params.rb` |
| 6 | 0.703 | `tell` | method | `sinatra-contrib/lib/sinatra/streaming.rb` |
| 7 | 0.699 | `errback` | method | `lib/sinatra/base.rb` |
| 8 | 0.696 | `uri` | method | `lib/sinatra/base.rb` |
| 9 | 0.692 | `@response_hash` | variable | `sinatra-contrib/lib/sinatra/cookies.rb` |
| 10 | 0.691 | `http_status` | method | `lib/sinatra/base.rb` |

Quality: diversity=0.4, same_kind=0.9, ns_overlap=0.0, unique_files=5

**Pivot:** `new` (method, ruby, refs=326)  
File: `rack-protection/lib/rack/protection.rb`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.831 | `new` | method | `lib/sinatra/base.rb` |
| 2 | 0.829 | `app` | method | `sinatra-contrib/lib/sinatra/namespace.rb` |
| 3 | 0.829 | `new` | method | `sinatra-contrib/lib/sinatra/extension.rb` |
| 4 | 0.815 | `new` | method | `sinatra-contrib/lib/sinatra/namespace.rb` |
| 5 | 0.813 | `new` | method | `lib/sinatra/base.rb` |
| 6 | 0.792 | `set` | method | `sinatra-contrib/lib/sinatra/cookies.rb` |
| 7 | 0.781 | `default_options` | method | `rack-protection/lib/rack/protection/base.rb` |
| 8 | 0.779 | `options` | method | `lib/sinatra/base.rb` |
| 9 | 0.766 | `default_options` | method | `rack-protection/lib/rack/protection/base.rb` |
| 10 | 0.766 | `app=` | method | `sinatra-contrib/lib/sinatra/test_helpers.rb` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=0.4, unique_files=6

### julie_316c0b08 (rust)

**Pivot:** `name` (method, rust, refs=3397)  
File: `src/tools/metrics/session.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.903 | `as_str` | method | `src/embeddings/mod.rs` |
| 2 | 0.815 | `from_name` | method | `src/tools/metrics/session.rs` |
| 3 | 0.790 | `find_method_name_node` | function | `crates/julie-extractors/src/powershell/helpers.rs` |
| 4 | 0.790 | `len` | method | `src/search/language_config.rs` |
| 5 | 0.790 | `is_static_method` | function | `crates/julie-extractors/src/dart/helpers.rs` |
| 6 | 0.780 | `extract_method_name_from_call` | function | `crates/julie-extractors/src/ruby/helpers.rs` |
| 7 | 0.779 | `handler` | namespace | `src/lib.rs` |
| 8 | 0.776 | `from_str` | method | `xtask/src/manifest.rs` |
| 9 | 0.775 | `extract_singleton_method_name` | function | `crates/julie-extractors/src/ruby/helpers.rs` |
| 10 | 0.765 | `from_string` | method | `crates/julie-extractors/src/base/types.rs` |

Quality: diversity=0.9, same_kind=0.5, ns_overlap=0.0, unique_files=9

**Pivot:** `new` (method, rust, refs=3196)  
File: `src/search/schema.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.826 | `SchemaFields` | struct | `src/search/schema.rs` |
| 2 | 0.789 | `initialize_schema` | method | `src/database/schema.rs` |
| 3 | 0.784 | `new` | method | `crates/julie-extractors/src/json/mod.rs` |
| 4 | 0.784 | `new` | method | `crates/julie-extractors/src/markdown/mod.rs` |
| 5 | 0.784 | `new` | method | `crates/julie-extractors/src/toml/mod.rs` |
| 6 | 0.784 | `new` | method | `crates/julie-extractors/src/yaml/mod.rs` |
| 7 | 0.778 | `new` | method | `crates/julie-extractors/src/html/mod.rs` |
| 8 | 0.778 | `new` | method | `crates/julie-extractors/src/java/mod.rs` |
| 9 | 0.778 | `new` | method | `crates/julie-extractors/src/javascript/mod.rs` |
| 10 | 0.778 | `new` | method | `crates/julie-extractors/src/kotlin/mod.rs` |

Quality: diversity=0.9, same_kind=0.9, ns_overlap=0.8, unique_files=10

**Pivot:** `new` (method, rust, refs=3196)  
File: `src/tools/get_context/allocation.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.806 | `TokenBudget` | struct | `src/tools/get_context/allocation.rs` |
| 2 | 0.784 | `truncate_to_token_budget` | function | `src/tools/get_context/content.rs` |
| 3 | 0.762 | `adaptive` | method | `src/tools/get_context/allocation.rs` |
| 4 | 0.749 | `new` | method | `crates/julie-extractors/src/json/mod.rs` |
| 5 | 0.749 | `new` | method | `crates/julie-extractors/src/markdown/mod.rs` |
| 6 | 0.749 | `new` | method | `crates/julie-extractors/src/toml/mod.rs` |
| 7 | 0.749 | `new` | method | `crates/julie-extractors/src/yaml/mod.rs` |
| 8 | 0.747 | `new` | method | `crates/julie-extractors/src/html/mod.rs` |
| 9 | 0.747 | `new` | method | `crates/julie-extractors/src/java/mod.rs` |
| 10 | 0.747 | `new` | method | `crates/julie-extractors/src/javascript/mod.rs` |

Quality: diversity=0.8, same_kind=0.8, ns_overlap=0.7, unique_files=9

**Pivot:** `new` (method, rust, refs=3195)  
File: `src/utils/token_estimation.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `new` | method | `src/tools/workspace/parser_pool.rs` |
| 2 | 1.000 | `new` | method | `crates/julie-extractors/src/manager.rs` |
| 3 | 1.000 | `new` | method | `src/utils/context_truncation.rs` |
| 4 | 1.000 | `new` | method | `src/tools/metrics/session.rs` |
| 5 | 0.953 | `new` | method | `src/workspace/mod.rs` |
| 6 | 0.907 | `new` | method | `crates/julie-extractors/src/json/mod.rs` |
| 7 | 0.907 | `new` | method | `crates/julie-extractors/src/markdown/mod.rs` |
| 8 | 0.907 | `new` | method | `crates/julie-extractors/src/toml/mod.rs` |
| 9 | 0.907 | `new` | method | `crates/julie-extractors/src/yaml/mod.rs` |
| 10 | 0.898 | `new` | method | `crates/julie-extractors/src/html/mod.rs` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=10

**Pivot:** `new` (method, rust, refs=3195)  
File: `src/database/mod.rs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.845 | `info` | function | `src/database/mod.rs` |
| 2 | 0.845 | `info` | function | `src/database/mod.rs` |
| 3 | 0.824 | `initialize_database` | method | `src/workspace/mod.rs` |
| 4 | 0.822 | `new` | method | `src/tracing/mod.rs` |
| 5 | 0.810 | `initialize_schema` | method | `src/database/schema.rs` |
| 6 | 0.801 | `debug` | function | `src/database/mod.rs` |
| 7 | 0.793 | `new` | method | `crates/julie-extractors/src/json/mod.rs` |
| 8 | 0.793 | `new` | method | `crates/julie-extractors/src/markdown/mod.rs` |
| 9 | 0.793 | `new` | method | `crates/julie-extractors/src/toml/mod.rs` |
| 10 | 0.793 | `new` | method | `crates/julie-extractors/src/yaml/mod.rs` |

Quality: diversity=0.7, same_kind=0.7, ns_overlap=0.5, unique_files=8

### cats_c701f713 (scala)

**Pivot:** `*` (method, scala, refs=2255)  
File: `algebra-core/src/main/scala/algebra/ring/Signed.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.977 | `**` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 2 | 0.858 | `unary_-` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 3 | 0.810 | `Sign` | class | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 4 | 0.810 | `sign` | method | `...a-2.12/cats/kernel/compat/scalaVersionSpecific.scala` |
| 5 | 0.807 | `one` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 6 | 0.804 | `sign` | method | `...a-2.12/cats/kernel/compat/scalaVersionSpecific.scala` |
| 7 | 0.793 | `sign` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 8 | 0.790 | `signed` | method | `...s/shared/src/main/scala/algebra/laws/OrderLaws.scala` |
| 9 | 0.786 | `sign` | method | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |
| 10 | 0.784 | `Sign` | class | `algebra-core/src/main/scala/algebra/ring/Signed.scala` |

Quality: diversity=0.3, same_kind=0.8, ns_overlap=0.0, unique_files=3

**Pivot:** `*` (method, scala, refs=2255)  
File: `laws/src/main/scala/cats/laws/discipline/MiniInt.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.992 | `/` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 2 | 0.991 | `|` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 3 | 0.983 | `+` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 4 | 0.880 | `toInt` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 5 | 0.847 | `fromInt` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 6 | 0.838 | `unary_-` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 7 | 0.837 | `wrapped` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 8 | 0.828 | `min` | method | `...in/scala/cats/kernel/instances/BigIntInstances.scala` |
| 9 | 0.825 | `compare` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |
| 10 | 0.819 | `inverse` | method | `laws/src/main/scala/cats/laws/discipline/MiniInt.scala` |

Quality: diversity=0.1, same_kind=1.0, ns_overlap=0.0, unique_files=2

**Pivot:** `map` (class, scala, refs=1472)  
File: `core/src/main/scala-2.12/cats/instances/package.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `map` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 2 | 0.948 | `map` | class | `alleycats-core/src/main/scala/alleycats/std/map.scala` |
| 3 | 0.893 | `stream` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 4 | 0.882 | `vector` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 5 | 0.882 | `vector` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 6 | 0.881 | `list` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 7 | 0.881 | `list` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 8 | 0.876 | `function` | class | `core/src/main/scala-2.12/cats/instances/package.scala` |
| 9 | 0.876 | `function` | class | `core/src/main/scala-2.13+/cats/instances/package.scala` |
| 10 | 0.868 | `MapInstances1` | trait | `algebra-core/src/main/scala/algebra/instances/map.scala` |

Quality: diversity=0.6, same_kind=0.9, ns_overlap=0.2, unique_files=4

**Pivot:** `map` (method, scala, refs=1472)  
File: `core/src/main/scala/cats/data/AndThen.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.944 | `map` | method | `free/src/main/scala/cats/free/FreeApplicative.scala` |
| 2 | 0.943 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 3 | 0.943 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 4 | 0.943 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 5 | 0.940 | `map` | method | `core/src/main/scala/cats/package.scala` |
| 6 | 0.937 | `map` | method | `core/src/main/scala/cats/instances/function.scala` |
| 7 | 0.936 | `map` | method | `alleycats-core/src/main/scala/alleycats/std/map.scala` |
| 8 | 0.936 | `map` | method | `core/src/main/scala/cats/instances/map.scala` |
| 9 | 0.935 | `map` | method | `core/src/main/scala/cats/data/EitherT.scala` |
| 10 | 0.935 | `map` | method | `core/src/main/scala/cats/instances/either.scala` |

Quality: diversity=1.0, same_kind=1.0, ns_overlap=1.0, unique_files=8

**Pivot:** `map` (method, scala, refs=1472)  
File: `core/src/main/scala/cats/Bifunctor.scala`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.953 | `map` | method | `core/src/main/scala/cats/Bifunctor.scala` |
| 2 | 0.934 | `map` | method | `core/src/main/scala/cats/data/Ior.scala` |
| 3 | 0.934 | `map` | method | `core/src/main/scala/cats/data/Ior.scala` |
| 4 | 0.934 | `map` | method | `core/src/main/scala/cats/data/Const.scala` |
| 5 | 0.934 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 6 | 0.934 | `map` | method | `core/src/main/scala/cats/instances/tuple.scala` |
| 7 | 0.933 | `map` | method | `core/src/main/scala/cats/Parallel.scala` |
| 8 | 0.933 | `map` | method | `core/src/main/scala/cats/Representable.scala` |
| 9 | 0.933 | `map` | method | `alleycats-core/src/main/scala/alleycats/Extract.scala` |
| 10 | 0.933 | `map` | method | `alleycats-core/src/main/scala/alleycats/Pure.scala` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=1.0, unique_files=8

### alamofire_3d4cceb5 (swift)

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.982 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.978 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.940 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.918 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.892 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.891 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.888 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.876 | `AlamofireNotifications` | class | `Source/Core/Notifications.swift` |
| 9 | 0.869 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.869 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.6, same_kind=0.9, ns_overlap=0.9, unique_files=2

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.982 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.976 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.953 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.912 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.896 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.886 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.883 | `requestDidSuspend` | method | `Source/Core/Notifications.swift` |
| 8 | 0.882 | `AlamofireNotifications` | class | `Source/Core/Notifications.swift` |
| 9 | 0.881 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.873 | `requestDidCancel` | method | `Source/Core/Notifications.swift` |

Quality: diversity=0.4, same_kind=0.9, ns_overlap=0.7, unique_files=2

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.953 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.941 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.940 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.930 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.924 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.912 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.908 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.897 | `request` | method | `Source/Features/EventMonitor.swift` |
| 9 | 0.896 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.892 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.7, same_kind=1.0, ns_overlap=1.0, unique_files=2

**Pivot:** `request` (method, swift, refs=546)  
File: `Source/Core/Notifications.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.978 | `request` | method | `Source/Core/Notifications.swift` |
| 2 | 0.976 | `request` | method | `Source/Core/Notifications.swift` |
| 3 | 0.941 | `request` | method | `Source/Core/Notifications.swift` |
| 4 | 0.917 | `request` | method | `Source/Features/EventMonitor.swift` |
| 5 | 0.891 | `request` | method | `Source/Features/EventMonitor.swift` |
| 6 | 0.889 | `request` | method | `Source/Features/EventMonitor.swift` |
| 7 | 0.884 | `request` | method | `Source/Features/EventMonitor.swift` |
| 8 | 0.877 | `requestDidCancel` | method | `Source/Core/Notifications.swift` |
| 9 | 0.875 | `AlamofireNotifications` | class | `Source/Core/Notifications.swift` |
| 10 | 0.864 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.5, same_kind=0.9, ns_overlap=0.8, unique_files=2

**Pivot:** `request` (method, swift, refs=544)  
File: `Source/Core/Session.swift`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.968 | `request` | method | `Source/Core/Session.swift` |
| 2 | 0.955 | `request` | method | `Source/Core/Session.swift` |
| 3 | 0.914 | `streamRequest` | method | `Source/Core/Session.swift` |
| 4 | 0.908 | `streamRequest` | method | `Source/Core/Session.swift` |
| 5 | 0.864 | `streamRequest` | method | `Source/Core/Session.swift` |
| 6 | 0.854 | `upload` | function | `Source/Core/Session.swift` |
| 7 | 0.823 | `download` | function | `Source/Core/Session.swift` |
| 8 | 0.822 | `request` | method | `Source/Features/EventMonitor.swift` |
| 9 | 0.819 | `request` | method | `Source/Features/EventMonitor.swift` |
| 10 | 0.817 | `request` | method | `Source/Features/EventMonitor.swift` |

Quality: diversity=0.3, same_kind=0.8, ns_overlap=0.5, unique_files=2

### labhandbookv2_67e8c1cf (typescript)

**Pivot:** `Error` (method, csharp, refs=84)  
File: `src/LabHandbook.Api/Models/Dto/ApiResponse.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.943 | `Error` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 2 | 0.847 | `ApiError` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 3 | 0.845 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 4 | 0.816 | `FieldError` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 5 | 0.814 | `ApiError` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 6 | 0.806 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 7 | 0.798 | `FieldError` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 8 | 0.787 | `Success` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 9 | 0.778 | `ApiResponse` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 10 | 0.761 | `Success` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |

Quality: diversity=0.3, same_kind=0.3, ns_overlap=0.1, unique_files=2

**Pivot:** `Error` (method, csharp, refs=84)  
File: `src/LabHandbook.Api/Models/Dto/ApiResponse.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.943 | `Error` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 2 | 0.877 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 3 | 0.827 | `ApiResponse` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 4 | 0.824 | `ApiError` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 5 | 0.812 | `Success` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 6 | 0.793 | `ApiError` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 7 | 0.792 | `ApiResponse` | interface | `src/LabHandbook.Api/ClientApp/src/types/api.ts` |
| 8 | 0.764 | `WriteErrorResponse` | method | `...Infrastructure/Middleware/ErrorHandlingMiddleware.cs` |
| 9 | 0.755 | `Success` | method | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |
| 10 | 0.746 | `FieldError` | class | `src/LabHandbook.Api/Models/Dto/ApiResponse.cs` |

Quality: diversity=0.3, same_kind=0.4, ns_overlap=0.1, unique_files=3

**Pivot:** `ToDto` (method, csharp, refs=56)  
File: `src/LabHandbook.Api/Models/Mapping/SectionMappingExtensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.881 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 2 | 0.860 | `ToDto` | method | `...dbook.Api/Models/Mapping/LabTestMappingExtensions.cs` |
| 3 | 0.859 | `ToDto` | method | `...dbook.Api/Models/Mapping/ContentMappingExtensions.cs` |
| 4 | 0.848 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 5 | 0.839 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 6 | 0.831 | `SectionDto` | class | `src/LabHandbook.Api/Models/Dto/SectionDto.cs` |
| 7 | 0.824 | `handleDelete` | function | `....Api/ClientApp/src/components/admin/SectionAdmin.vue` |
| 8 | 0.822 | `SectionMappingExtensions` | class | `...dbook.Api/Models/Mapping/SectionMappingExtensions.cs` |
| 9 | 0.813 | `ToDto` | method | `...book.Api/Models/Mapping/CalendarMappingExtensions.cs` |
| 10 | 0.807 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |

Quality: diversity=0.9, same_kind=0.7, ns_overlap=0.7, unique_files=8

**Pivot:** `ToDto` (method, csharp, refs=56)  
File: `src/LabHandbook.Api/Models/Mapping/PageMappingExtensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.948 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 2 | 0.859 | `ToDto` | method | `...dbook.Api/Models/Mapping/ContentMappingExtensions.cs` |
| 3 | 0.848 | `ToDto` | method | `...dbook.Api/Models/Mapping/SectionMappingExtensions.cs` |
| 4 | 0.817 | `PageLinkDto` | class | `src/LabHandbook.Api/Models/Dto/PageLinkDto.cs` |
| 5 | 0.808 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 6 | 0.808 | `ToDto` | method | `...book.Api/Models/Mapping/CalendarMappingExtensions.cs` |
| 7 | 0.802 | `PageLinkDto` | interface | `src/LabHandbook.Api/ClientApp/src/types/pages.ts` |
| 8 | 0.794 | `openEditForm` | function | `...Api/ClientApp/src/components/cms/CmsDocumentList.vue` |
| 9 | 0.794 | `openEditForm` | function | `...ook.Api/ClientApp/src/components/cms/CmsLinkList.vue` |
| 10 | 0.793 | `PageMappingExtensions` | class | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |

Quality: diversity=0.8, same_kind=0.5, ns_overlap=0.5, unique_files=9

**Pivot:** `ToDto` (method, csharp, refs=56)  
File: `src/LabHandbook.Api/Models/Mapping/MediaMappingExtensions.cs`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.818 | `MediaFileDto` | class | `src/LabHandbook.Api/Models/Dto/MediaFileDto.cs` |
| 2 | 0.809 | `ToDto` | method | `...dbook.Api/Models/Mapping/ContentMappingExtensions.cs` |
| 3 | 0.809 | `MediaFileDto` | interface | `src/LabHandbook.Api/ClientApp/src/types/media.ts` |
| 4 | 0.803 | `MediaMappingExtensions` | class | `...andbook.Api/Models/Mapping/MediaMappingExtensions.cs` |
| 5 | 0.799 | `onEditFile` | function | `...ok.Api/ClientApp/src/components/admin/MediaAdmin.vue` |
| 6 | 0.786 | `ToDto` | method | `...Handbook.Api/Models/Mapping/PageMappingExtensions.cs` |
| 7 | 0.780 | `ToDto` | method | `...dbook.Api/Models/Mapping/SectionMappingExtensions.cs` |
| 8 | 0.776 | `MediaFile` | class | `src/LabHandbook.Api/Models/Domain/MediaFile.cs` |
| 9 | 0.767 | `ToDto` | method | `...Handbook.Api/Models/Mapping/UserMappingExtensions.cs` |
| 10 | 0.763 | `ToDto` | method | `...book.Api/Models/Mapping/CalendarMappingExtensions.cs` |

Quality: diversity=0.9, same_kind=0.5, ns_overlap=0.5, unique_files=10

### zod_df52de88 (typescript)

**Pivot:** `parse` (method, typescript, refs=3156)  
File: `packages/bench/safeparse.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.818 | `parse` | method | `packages/zod/src/v3/types.ts` |
| 2 | 0.811 | `_parse` | function | `packages/zod/src/v4/core/parse.ts` |
| 3 | 0.801 | `get` | method | `packages/zod/src/v4/classic/errors.ts` |
| 4 | 0.801 | `get` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 5 | 0.801 | `get` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 6 | 0.801 | `get` | method | `packages/zod/src/v4/core/util.ts` |
| 7 | 0.798 | `value` | method | `packages/bench/lazy-box.ts` |
| 8 | 0.798 | `value` | method | `packages/bench/lazy-box.ts` |
| 9 | 0.798 | `value` | method | `packages/bench/lazy-box.ts` |
| 10 | 0.798 | `value` | method | `packages/bench/property-access.ts` |

Quality: diversity=1.0, same_kind=0.9, ns_overlap=0.1, unique_files=7

**Pivot:** `parse` (method, typescript, refs=3154)  
File: `packages/zod/src/v4/mini/schemas.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.996 | `parse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 2 | 0.896 | `safeParse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 3 | 0.894 | `safeParse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 4 | 0.887 | `parseAsync` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 5 | 0.887 | `parseAsync` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 6 | 0.873 | `decode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 7 | 0.832 | `parse` | method | `packages/zod/src/v3/types.ts` |
| 8 | 0.821 | `encode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 9 | 0.796 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 10 | 0.793 | `decodeAsync` | method | `packages/zod/src/v4/classic/schemas.ts` |

Quality: diversity=0.8, same_kind=1.0, ns_overlap=0.2, unique_files=3

**Pivot:** `parse` (method, typescript, refs=3153)  
File: `packages/zod/src/v4/classic/schemas.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.996 | `parse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 2 | 0.892 | `safeParse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 3 | 0.890 | `safeParse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 4 | 0.882 | `parseAsync` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 5 | 0.882 | `parseAsync` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 6 | 0.871 | `decode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 7 | 0.839 | `parse` | method | `packages/zod/src/v3/types.ts` |
| 8 | 0.821 | `encode` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 9 | 0.803 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 10 | 0.792 | `parsedType` | function | `packages/zod/src/v4/core/util.ts` |

Quality: diversity=0.6, same_kind=0.9, ns_overlap=0.2, unique_files=4

**Pivot:** `parse` (method, typescript, refs=3152)  
File: `packages/zod/src/v3/types.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.839 | `parse` | method | `packages/zod/src/v4/classic/schemas.ts` |
| 2 | 0.838 | `parseAsync` | method | `packages/zod/src/v3/types.ts` |
| 3 | 0.832 | `parse` | method | `packages/zod/src/v4/mini/schemas.ts` |
| 4 | 0.829 | `safeParse` | method | `packages/zod/src/v3/types.ts` |
| 5 | 0.818 | `parse` | method | `packages/bench/safeparse.ts` |
| 6 | 0.805 | `ParseParams` | type | `packages/zod/src/v3/helpers/parseUtil.ts` |
| 7 | 0.799 | `_parse` | function | `packages/zod/src/v4/core/parse.ts` |
| 8 | 0.798 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 9 | 0.798 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 10 | 0.798 | `_parse` | method | `packages/zod/src/v3/types.ts` |

Quality: diversity=0.5, same_kind=0.8, ns_overlap=0.3, unique_files=6

**Pivot:** `parse` (method, typescript, refs=3102)  
File: `packages/zod/src/v4/core/schemas.ts`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.913 | `run` | method | `packages/zod/src/v4/core/schemas.ts` |
| 2 | 0.809 | `check` | method | `packages/zod/src/v4/core/checks.ts` |
| 3 | 0.787 | `ParsePayload` | interface | `packages/zod/src/v4/core/schemas.ts` |
| 4 | 0.772 | `handleTupleResult` | function | `packages/zod/src/v4/core/schemas.ts` |
| 5 | 0.771 | `handleReadonlyResult` | function | `packages/zod/src/v4/core/schemas.ts` |
| 6 | 0.761 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 7 | 0.761 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 8 | 0.761 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 9 | 0.761 | `_parse` | method | `packages/zod/src/v3/types.ts` |
| 10 | 0.761 | `handleArrayResult` | function | `packages/zod/src/v4/core/schemas.ts` |

Quality: diversity=0.5, same_kind=0.6, ns_overlap=0.0, unique_files=3

### zls_4b29ec8b (zig)

**Pivot:** `allocator` (method, zig, refs=690)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.954 | `allocator` | method | `src/testing.zig` |
| 2 | 0.881 | `tracyAllocator` | function | `src/tracy.zig` |
| 3 | 0.877 | `deinit` | method | `src/analyser/segmented_list.zig` |
| 4 | 0.874 | `init` | method | `src/tracy.zig` |
| 5 | 0.856 | `append` | method | `src/analyser/segmented_list.zig` |
| 6 | 0.852 | `deinit` | method | `src/analysis.zig` |
| 7 | 0.840 | `addOne` | method | `src/analyser/segmented_list.zig` |
| 8 | 0.837 | `deinit` | method | `src/analyser/string_pool.zig` |
| 9 | 0.827 | `deinit` | method | `src/ast.zig` |
| 10 | 0.824 | `deinit` | method | `src/DocumentStore.zig` |

Quality: diversity=0.8, same_kind=0.9, ns_overlap=0.1, unique_files=7

**Pivot:** `end` (method, zig, refs=485)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `end` | method | `src/tracy.zig` |
| 2 | 0.955 | `end` | method | `src/tracy.zig` |
| 3 | 0.815 | `append` | method | `src/ast.zig` |
| 4 | 0.791 | `addText` | method | `src/tracy.zig` |
| 5 | 0.791 | `addText` | method | `src/tracy.zig` |
| 6 | 0.784 | `next` | method | `src/analyser/segmented_list.zig` |
| 7 | 0.782 | `finish` | method | `src/features/semantic_tokens.zig` |
| 8 | 0.769 | `set` | method | `src/analyser/segmented_list.zig` |
| 9 | 0.768 | `append` | method | `src/features/completions.zig` |
| 10 | 0.764 | `hash` | method | `src/analysis.zig` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=0.2, unique_files=6

**Pivot:** `end` (method, zig, refs=485)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `end` | method | `src/tracy.zig` |
| 2 | 0.955 | `end` | method | `src/tracy.zig` |
| 3 | 0.815 | `append` | method | `src/ast.zig` |
| 4 | 0.791 | `addText` | method | `src/tracy.zig` |
| 5 | 0.791 | `addText` | method | `src/tracy.zig` |
| 6 | 0.784 | `next` | method | `src/analyser/segmented_list.zig` |
| 7 | 0.782 | `finish` | method | `src/features/semantic_tokens.zig` |
| 8 | 0.769 | `set` | method | `src/analyser/segmented_list.zig` |
| 9 | 0.768 | `append` | method | `src/features/completions.zig` |
| 10 | 0.764 | `hash` | method | `src/analysis.zig` |

Quality: diversity=0.6, same_kind=1.0, ns_overlap=0.2, unique_files=6

**Pivot:** `end` (method, zig, refs=485)  
File: `src/tracy.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 0.955 | `end` | method | `src/tracy.zig` |
| 2 | 0.955 | `end` | method | `src/tracy.zig` |
| 3 | 0.815 | `append` | method | `src/ast.zig` |
| 4 | 0.788 | `next` | method | `src/analyser/segmented_list.zig` |
| 5 | 0.784 | `hash` | method | `src/Uri.zig` |
| 6 | 0.784 | `finish` | method | `src/features/semantic_tokens.zig` |
| 7 | 0.775 | `hash` | method | `src/features/code_actions.zig` |
| 8 | 0.772 | `next` | method | `src/analysis.zig` |
| 9 | 0.769 | `set` | method | `src/analyser/segmented_list.zig` |
| 10 | 0.769 | `hash` | method | `src/analysis.zig` |

Quality: diversity=0.8, same_kind=1.0, ns_overlap=0.2, unique_files=7

**Pivot:** `Index` (enum, zig, refs=430)  
File: `src/TrigramStore.zig`

| # | Sim | Name | Kind | File |
|---|-----|------|------|------|
| 1 | 1.000 | `Index` | enum | `src/analyser/InternPool.zig` |
| 2 | 1.000 | `Index` | enum | `src/analyser/InternPool.zig` |
| 3 | 1.000 | `Index` | enum | `src/analyser/InternPool.zig` |
| 4 | 0.979 | `Index` | enum | `src/DocumentScope.zig` |
| 5 | 0.979 | `Index` | enum | `src/analyser/InternPool.zig` |
| 6 | 0.957 | `Index` | enum | `src/DocumentScope.zig` |
| 7 | 0.917 | `NamespaceIndex` | enum | `src/analyser/InternPool.zig` |
| 8 | 0.888 | `BucketIndex` | enum | `src/TrigramStore.zig` |
| 9 | 0.871 | `OptionalIndex` | enum | `src/DocumentScope.zig` |
| 10 | 0.868 | `OptionalIndex` | enum | `src/analyser/InternPool.zig` |

Quality: diversity=0.9, same_kind=1.0, ns_overlap=0.6, unique_files=3
