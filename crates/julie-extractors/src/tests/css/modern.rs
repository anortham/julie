use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_grid_flexbox_and_modern_layout_properties() {
        let css_code = r#"
/* CSS Grid Layout */
.grid-container {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  grid-template-rows: auto 1fr auto;
  grid-template-areas:
    "header header header"
    "sidebar main aside"
    "footer footer footer";
  gap: 2rem;
  min-height: 100vh;
}

.grid-header {
  grid-area: header;
  background-color: #3498db;
  padding: 2rem;
}

.grid-sidebar {
  grid-area: sidebar;
  background-color: #ecf0f1;
  padding: 1rem;
}

.grid-main {
  grid-area: main;
  padding: 2rem;
}

.grid-aside {
  grid-area: aside;
  background-color: #f8f9fa;
  padding: 1rem;
}

.grid-footer {
  grid-area: footer;
  background-color: #2c3e50;
  color: white;
  padding: 2rem;
  text-align: center;
}

.photo-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  grid-auto-rows: 200px;
  grid-auto-flow: dense;
  gap: 1rem;
}

.photo-item:nth-child(3n) {
  grid-column: span 2;
  grid-row: span 2;
}

.photo-item:nth-child(5n) {
  grid-column: span 1;
  grid-row: span 3;
}

.flex-container {
  display: flex;
  flex-direction: row;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  align-content: flex-start;
  gap: 1rem;
}

.flex-item {
  flex: 1 1 auto;
  min-width: 0;
}

.flex-item-grow {
  flex-grow: 2;
  flex-shrink: 1;
  flex-basis: 200px;
}

.flex-item-fixed {
  flex: 0 0 150px;
}

.nav-flex {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 1rem 2rem;
}

.nav-flex .logo {
  flex: 0 0 auto;
}

.nav-flex .menu {
  display: flex;
  list-style: none;
  margin: 0;
  padding: 0;
  gap: 2rem;
}

.nav-flex .actions {
  display: flex;
  gap: 1rem;
  margin-left: auto;
}

.sticky-header {
  position: sticky;
  top: 0;
  z-index: 100;
  background-color: rgba(255, 255, 255, 0.95);
  backdrop-filter: blur(10px);
}

.fixed-sidebar {
  position: fixed;
  top: 0;
  left: 0;
  width: 280px;
  height: calc(100vh - 80px);
  overflow-y: auto;
}

.absolute-overlay {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
}

.subgrid-container {
  display: grid;
  grid-template-columns: repeat(12, 1fr);
}

.subgrid-item {
  display: grid;
  grid-template-columns: subgrid;
}

.card-container {
  container-type: inline-size;
  container-name: card;
}

@container card (min-width: 400px) {
  .card-content {
    display: flex;
    gap: 1rem;
  }

  .card-image {
    flex: 0 0 150px;
  }
}

@container card (min-width: 600px) {
  .card-content {
    flex-direction: column;
  }

  .card-image {
    flex: none;
    width: 100%;
    height: 200px;
  }
}
"#;

        let symbols = extract_symbols(css_code);

        let grid_container = symbols.iter().find(|s| s.name == ".grid-container");
        assert!(grid_container.is_some());
        assert!(
            grid_container
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("display: grid")
        );

        let grid_header = symbols.iter().find(|s| s.name == ".grid-header");
        assert!(grid_header.is_some());

        let photo_grid = symbols.iter().find(|s| s.name == ".photo-grid");
        assert!(photo_grid.is_some());
        assert!(
            photo_grid
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("grid-auto-flow: dense")
        );

        let photo_item = symbols
            .iter()
            .find(|s| s.name == ".photo-item:nth-child(3n)");
        assert!(photo_item.is_some());

        let flex_container = symbols.iter().find(|s| s.name == ".flex-container");
        assert!(flex_container.is_some());
        assert!(
            flex_container
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("display: flex")
        );

        let flex_item = symbols.iter().find(|s| s.name == ".flex-item");
        assert!(flex_item.is_some());

        let flex_item_grow = symbols.iter().find(|s| s.name == ".flex-item-grow");
        assert!(flex_item_grow.is_some());

        let nav_flex = symbols.iter().find(|s| s.name == ".nav-flex");
        assert!(nav_flex.is_some());

        let sticky_header = symbols.iter().find(|s| s.name == ".sticky-header");
        assert!(sticky_header.is_some());

        let fixed_sidebar = symbols.iter().find(|s| s.name == ".fixed-sidebar");
        assert!(fixed_sidebar.is_some());

        let absolute_overlay = symbols.iter().find(|s| s.name == ".absolute-overlay");
        assert!(absolute_overlay.is_some());

        let subgrid_container = symbols.iter().find(|s| s.name == ".subgrid-container");
        assert!(subgrid_container.is_some());

        let subgrid_item = symbols.iter().find(|s| s.name == ".subgrid-item");
        assert!(subgrid_item.is_some());

        let card_container = symbols.iter().find(|s| s.name == ".card-container");
        assert!(card_container.is_some());

        let container_query_400 = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@container card (min-width: 400px)")
        });
        assert!(container_query_400.is_some());

        let container_query_600 = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .unwrap()
                .contains("@container card (min-width: 600px)")
        });
        assert!(container_query_600.is_some());
    }
}
