// R Real-World Validation Tests
// Tests against actual GitHub repositories (ggplot2, dplyr)

use super::*;
use crate::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ggplot2_geom_point_pattern() {
        // Simplified pattern from ggplot2/R/geom-point.r
        let r_code = r#"
#' Points
#'
#' The point geom is used to create scatterplots.
#'
#' @export
geom_point <- function(mapping = NULL, data = NULL,
                       stat = "identity", position = "identity",
                       ...,
                       na.rm = FALSE,
                       show.legend = NA,
                       inherit.aes = TRUE) {
  layer(
    data = data,
    mapping = mapping,
    stat = stat,
    geom = GeomPoint,
    position = position,
    show.legend = show.legend,
    inherit.aes = inherit.aes,
    params = list(
      na.rm = na.rm,
      ...
    )
  )
}

#' @rdname geom_point
#' @export
GeomPoint <- ggproto("GeomPoint", Geom,
  required_aes = c("x", "y"),
  non_missing_aes = c("size", "shape", "colour"),
  default_aes = aes(
    shape = 19, colour = "black", size = 1.5, fill = NA,
    alpha = NA, stroke = 0.5
  ),

  draw_panel = function(data, panel_params, coord, na.rm = FALSE) {
    if (is.character(data$shape)) {
      data$shape <- translate_shape_string(data$shape)
    }

    coords <- coord$transform(data, panel_params)

    ggplot2:::ggname("geom_point",
      pointsGrob(
        coords$x, coords$y,
        pch = coords$shape,
        gp = gpar(
          col = alpha(coords$colour, coords$alpha),
          fill = alpha(coords$fill, coords$alpha),
          fontsize = coords$size * .pt + coords$stroke * .stroke / 2,
          lwd = coords$stroke * .stroke / 2
        )
      )
    )
  }
)
"#;

        let symbols = extract_symbols(r_code);

        // Should extract geom_point function
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(
            functions.len() >= 1,
            "Should extract geom_point and GeomPoint functions"
        );

        // Verify geom_point function exists
        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(
            func_names.contains(&"geom_point"),
            "Should find geom_point function"
        );
    }

    #[test]
    fn test_dplyr_filter_pattern() {
        // Simplified pattern from dplyr/R/filter.R
        let r_code = r#"
#' Keep rows that match a condition
#'
#' The `filter()` function is used to subset a data frame,
#' retaining all rows that satisfy your conditions.
#'
#' @export
filter <- function(.data, ..., .preserve = FALSE) {
  UseMethod("filter")
}

#' @export
filter.data.frame <- function(.data, ..., .preserve = FALSE) {
  dots <- quos(...)

  if (length(dots) == 0) {
    return(.data)
  }

  # Evaluate conditions
  rows <- rep(TRUE, nrow(.data))
  for (i in seq_along(dots)) {
    condition <- eval_tidy(dots[[i]], .data)
    rows <- rows & condition
  }

  # Apply filter
  .data[rows, , drop = FALSE]
}

#' Select/rename variables by name
#'
#' `select()` keeps only the variables you mention; `rename()`
#' keeps all variables.
#'
#' @export
select <- function(.data, ...) {
  UseMethod("select")
}

#' @export
select.data.frame <- function(.data, ...) {
  dots <- quos(...)

  # Extract column names
  cols <- tidyselect::vars_select(names(.data), !!!dots)

  # Subset columns
  .data[, cols, drop = FALSE]
}

#' Create, modify, and delete columns
#'
#' `mutate()` adds new variables and preserves existing ones.
#'
#' @export
mutate <- function(.data, ...) {
  UseMethod("mutate")
}

#' @export
mutate.data.frame <- function(.data, ...) {
  dots <- enquos(...)

  for (i in seq_along(dots)) {
    name <- names(dots)[[i]]
    value <- eval_tidy(dots[[i]], .data)
    .data[[name]] <- value
  }

  .data
}
"#;

        let symbols = extract_symbols(r_code);

        // Should extract multiple dplyr verbs
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(
            functions.len() >= 6,
            "Should extract filter, select, mutate and their methods"
        );

        // Verify key dplyr functions exist
        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(
            func_names.contains(&"filter"),
            "Should find filter function"
        );
        assert!(
            func_names.contains(&"select"),
            "Should find select function"
        );
        assert!(
            func_names.contains(&"mutate"),
            "Should find mutate function"
        );
    }

    #[test]
    fn test_complex_real_world_pipeline() {
        // Realistic data analysis pipeline
        let r_code = r#"
library(tidyverse)

# Load and clean data
clean_data <- read_csv("raw_data.csv") %>%
  filter(!is.na(value), value > 0) %>%
  mutate(
    log_value = log(value),
    category = factor(category, levels = c("A", "B", "C"))
  ) %>%
  group_by(category, year) %>%
  summarize(
    mean_value = mean(value),
    median_value = median(value),
    n = n(),
    .groups = "drop"
  )

# Statistical analysis
model_results <- clean_data %>%
  nest(data = -category) %>%
  mutate(
    model = map(data, ~ lm(mean_value ~ year, data = .x)),
    tidy = map(model, broom::tidy),
    glance = map(model, broom::glance)
  ) %>%
  unnest(tidy)

# Visualization
final_plot <- ggplot(clean_data, aes(x = year, y = mean_value, color = category)) +
  geom_point(size = 3, alpha = 0.7) +
  geom_smooth(method = "lm", se = TRUE) +
  facet_wrap(~category, scales = "free_y") +
  labs(
    title = "Trend Analysis by Category",
    x = "Year",
    y = "Mean Value",
    color = "Category"
  ) +
  theme_minimal() +
  theme(
    legend.position = "bottom",
    plot.title = element_text(hjust = 0.5, size = 16, face = "bold")
  )

# Save results
write_csv(model_results, "analysis_results.csv")
ggsave("trend_plot.png", final_plot, width = 10, height = 6, dpi = 300)
"#;

        let symbols = extract_symbols(r_code);

        // Should extract all major variables
        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 3,
            "Should extract clean_data, model_results, final_plot"
        );

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"clean_data"), "Should find clean_data");
        assert!(
            var_names.contains(&"model_results"),
            "Should find model_results"
        );
        assert!(var_names.contains(&"final_plot"), "Should find final_plot");
    }

    #[test]
    fn test_shiny_app_pattern() {
        // Simplified Shiny app pattern (common in R web apps)
        let r_code = r#"
library(shiny)

# Define UI
ui <- fluidPage(
  titlePanel("Data Analysis Dashboard"),

  sidebarLayout(
    sidebarPanel(
      selectInput("variable", "Select Variable:",
                  choices = c("Sales", "Profit", "Quantity")),

      sliderInput("bins", "Number of bins:",
                  min = 1, max = 50, value = 30)
    ),

    mainPanel(
      plotOutput("distPlot"),
      tableOutput("summary")
    )
  )
)

# Define server logic
server <- function(input, output) {

  # Reactive data
  filtered_data <- reactive({
    data %>%
      filter(category == input$variable)
  })

  # Render plot
  output$distPlot <- renderPlot({
    ggplot(filtered_data(), aes(x = value)) +
      geom_histogram(bins = input$bins, fill = "steelblue") +
      theme_minimal()
  })

  # Render table
  output$summary <- renderTable({
    filtered_data() %>%
      summarize(
        Mean = mean(value),
        Median = median(value),
        SD = sd(value)
      )
  })
}

# Run the application
shinyApp(ui = ui, server = server)
"#;

        let symbols = extract_symbols(r_code);

        // Should extract ui and server
        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(variables.len() >= 1, "Should extract ui");
        assert!(functions.len() >= 1, "Should extract server function");

        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"ui"), "Should find ui definition");

        let func_names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
        assert!(
            func_names.contains(&"server"),
            "Should find server function"
        );
    }
}
