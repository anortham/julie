use crate::base::{EmbeddedSpanOffset, NormalizedSpan};

#[test]
fn test_embedded_range_helper_preserves_offsets() {
    let inline_host = "<style>.card { color: red; }</style>";
    let inline_start = inline_host.find(".card").unwrap();
    let inline_offset = EmbeddedSpanOffset::from_host_byte(inline_host, inline_start).unwrap();
    let inline_span = NormalizedSpan {
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 5,
        start_byte: 0,
        end_byte: 5,
    };

    assert_host_span(inline_host, inline_offset.apply(inline_span), ".card");

    let multiline_host = "<script>\n  function greet(name) {\n    return name;\n  }\n</script>";
    let function_start = multiline_host.find("function greet").unwrap();
    let function_end = multiline_host.find("  }\n</script>").unwrap() + "  }".len();
    let multiline_offset = EmbeddedSpanOffset::from_host_byte(multiline_host, function_start)
        .expect("function starts at a valid host byte");
    let local_function = &multiline_host[function_start..function_end];
    let multiline_span = NormalizedSpan {
        start_line: 1,
        start_column: 0,
        end_line: 3,
        end_column: 3,
        start_byte: 0,
        end_byte: local_function.len() as u32,
    };

    assert_host_span(
        multiline_host,
        multiline_offset.apply(multiline_span),
        local_function,
    );
}

fn assert_host_span(host: &str, span: NormalizedSpan, expected: &str) {
    assert_eq!(
        &host[span.start_byte as usize..span.end_byte as usize],
        expected
    );
    assert_eq!(
        (span.start_line, span.start_column),
        line_column_for_byte(host, span.start_byte as usize)
    );
    assert_eq!(
        (span.end_line, span.end_column),
        line_column_for_byte(host, span.end_byte as usize)
    );
}

fn line_column_for_byte(content: &str, target: usize) -> (u32, u32) {
    let prefix = &content[..target];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit_once('\n')
        .map(|(_, tail)| tail.len())
        .unwrap_or(prefix.len()) as u32;
    (line, column)
}
