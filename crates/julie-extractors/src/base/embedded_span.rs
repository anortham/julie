use super::span::NormalizedSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedSpanOffset {
    line_delta: u32,
    byte_delta: u32,
    first_line_column_delta: u32,
}

impl EmbeddedSpanOffset {
    pub fn from_host_byte(host_content: &str, byte_offset: usize) -> Option<Self> {
        let prefix = host_content.get(..byte_offset)?;
        let line_delta = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
        let first_line_column_delta = prefix
            .rsplit_once('\n')
            .map(|(_, tail)| tail.len())
            .unwrap_or(prefix.len()) as u32;

        Some(Self {
            line_delta,
            byte_delta: byte_offset as u32,
            first_line_column_delta,
        })
    }

    pub fn apply(self, span: NormalizedSpan) -> NormalizedSpan {
        NormalizedSpan {
            start_line: span.start_line + self.line_delta,
            start_column: if span.start_line == 1 {
                span.start_column + self.first_line_column_delta
            } else {
                span.start_column
            },
            end_line: span.end_line + self.line_delta,
            end_column: if span.end_line == 1 {
                span.end_column + self.first_line_column_delta
            } else {
                span.end_column
            },
            start_byte: span.start_byte + self.byte_delta,
            end_byte: span.end_byte + self.byte_delta,
        }
    }
}
