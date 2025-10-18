use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_responsive_utilities_and_flexible_layouts() {
        let css_code = r#"
/* Flexbox utilities */
.flex {
  display: flex;
}

.flex-row {
  flex-direction: row;
}

.flex-column {
  flex-direction: column;
}

.flex-wrap {
  flex-wrap: wrap;
}

.flex-nowrap {
  flex-wrap: nowrap;
}

.justify-start {
  justify-content: flex-start;
}

.justify-center {
  justify-content: center;
}

.justify-end {
  justify-content: flex-end;
}

.justify-between {
  justify-content: space-between;
}

.align-start {
  align-items: flex-start;
}

.align-center {
  align-items: center;
}

.align-end {
  align-items: flex-end;
}

/* Grid utilities */
.grid {
  display: grid;
}

.grid-cols-1 {
  grid-template-columns: repeat(1, minmax(0, 1fr));
}

.grid-cols-2 {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.grid-cols-3 {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}

.grid-cols-4 {
  grid-template-columns: repeat(4, minmax(0, 1fr));
}

.grid-rows-2 {
  grid-template-rows: repeat(2, minmax(0, 1fr));
}

.grid-rows-3 {
  grid-template-rows: repeat(3, minmax(0, 1fr));
}

.gap-4 {
  gap: 1rem;
}

.gap-8 {
  gap: 2rem;
}

/* Spacing utilities */
.m-0 { margin: 0; }
.m-1 { margin: 0.25rem; }
.m-2 { margin: 0.5rem; }
.m-3 { margin: 1rem; }
.m-4 { margin: 1.5rem; }

.p-0 { padding: 0; }
.p-1 { padding: 0.25rem; }
.p-2 { padding: 0.5rem; }
.p-3 { padding: 1rem; }
.p-4 { padding: 1.5rem; }

/* Width utilities */
.w-full { width: 100%; }
.w-1/2 { width: 50%; }
.w-1/3 { width: 33.333333%; }
.w-1/4 { width: 25%; }
.w-auto { width: auto; }

/* Height utilities */
.h-full { height: 100%; }
.h-screen { height: 100vh; }
.h-64 { height: 16rem; }

/* Text utilities */
.text-left { text-align: left; }
.text-center { text-align: center; }
.text-right { text-align: right; }

.text-sm { font-size: 0.875rem; }
.text-base { font-size: 1rem; }
.text-lg { font-size: 1.125rem; }
.text-xl { font-size: 1.25rem; }

.font-bold { font-weight: 700; }
.font-normal { font-weight: 400; }
.font-light { font-weight: 300; }

/* Color utilities */
.text-black { color: #000000; }
.text-white { color: #ffffff; }
.text-gray-500 { color: #6b7280; }
.text-blue-500 { color: #3b82f6; }
.text-red-500 { color: #ef4444; }

.bg-black { background-color: #000000; }
.bg-white { background-color: #ffffff; }
.bg-gray-100 { background-color: #f3f4f6; }
.bg-blue-500 { background-color: #3b82f6; }

/* Border utilities */
.border { border-width: 1px; }
.border-2 { border-width: 2px; }
.border-0 { border-width: 0; }

.border-solid { border-style: solid; }
.border-dashed { border-style: dashed; }

.border-gray-300 { border-color: #d1d5db; }
.border-blue-500 { border-color: #3b82f6; }

/* Responsive breakpoints */
@media (min-width: 640px) {
  .sm\\:block { display: block; }
  .sm\\:hidden { display: none; }
  .sm\\:flex { display: flex; }
}

@media (min-width: 768px) {
  .md\\:block { display: block; }
  .md\\:grid { display: grid; }
  .md\\:grid-cols-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}

@media (min-width: 1024px) {
  .lg\\:block { display: block; }
  .lg\\:grid-cols-3 { grid-template-columns: repeat(3, minmax(0, 1fr)); }
}
"#;

        let symbols = extract_symbols(css_code);

        // Test flexbox utilities
        let flex = symbols.iter().find(|s| s.name == ".flex");
        assert!(flex.is_some());
        assert_eq!(flex.unwrap().kind, SymbolKind::Class);

        let flex_row = symbols.iter().find(|s| s.name == ".flex-row");
        assert!(flex_row.is_some());

        let justify_center = symbols.iter().find(|s| s.name == ".justify-center");
        assert!(justify_center.is_some());

        let align_center = symbols.iter().find(|s| s.name == ".align-center");
        assert!(align_center.is_some());

        // Test grid utilities
        let grid = symbols.iter().find(|s| s.name == ".grid");
        assert!(grid.is_some());

        let grid_cols_3 = symbols.iter().find(|s| s.name == ".grid-cols-3");
        assert!(grid_cols_3.is_some());

        let gap_4 = symbols.iter().find(|s| s.name == ".gap-4");
        assert!(gap_4.is_some());

        // Test spacing utilities
        let m_0 = symbols.iter().find(|s| s.name == ".m-0");
        assert!(m_0.is_some());

        let p_3 = symbols.iter().find(|s| s.name == ".p-3");
        assert!(p_3.is_some());

        // Test width/height utilities
        let w_full = symbols.iter().find(|s| s.name == ".w-full");
        assert!(w_full.is_some());

        let h_screen = symbols.iter().find(|s| s.name == ".h-screen");
        assert!(h_screen.is_some());

        // Test text utilities
        let text_center = symbols.iter().find(|s| s.name == ".text-center");
        assert!(text_center.is_some());

        let text_lg = symbols.iter().find(|s| s.name == ".text-lg");
        assert!(text_lg.is_some());

        let font_bold = symbols.iter().find(|s| s.name == ".font-bold");
        assert!(font_bold.is_some());

        // Test color utilities
        let text_blue_500 = symbols.iter().find(|s| s.name == ".text-blue-500");
        assert!(text_blue_500.is_some());

        let bg_gray_100 = symbols.iter().find(|s| s.name == ".bg-gray-100");
        assert!(bg_gray_100.is_some());

        // Test border utilities
        let border = symbols.iter().find(|s| s.name == ".border");
        assert!(border.is_some());

        let border_dashed = symbols.iter().find(|s| s.name == ".border-dashed");
        assert!(border_dashed.is_some());
    }
}
