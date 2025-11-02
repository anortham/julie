# Real-world R code from ggplot2 package
# Source: https://github.com/tidyverse/ggplot2
# Simplified version of geom_point implementation

#' Points
#'
#' The point geom is used to create scatterplots. The scatterplot is most
#' useful for displaying the relationship between two continuous variables.
#'
#' @eval rd_aesthetics("geom", "point")
#' @export
#' @inheritParams layer
#' @inheritParams geom_bar
#' @examples
#' p <- ggplot(mtcars, aes(wt, mpg))
#' p + geom_point()
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

#' @rdname ggplot2-ggproto
#' @format NULL
#' @usage NULL
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
    stroke_size <- coords$stroke
    stroke_size[is.na(stroke_size)] <- 0

    ggname("geom_point",
      pointsGrob(
        coords$x, coords$y,
        pch = coords$shape,
        gp = gpar(
          col = alpha(coords$colour, coords$alpha),
          fill = alpha(coords$fill, coords$alpha),
          fontsize = coords$size * .pt + stroke_size * .stroke / 2,
          lwd = coords$stroke * .stroke / 2
        )
      )
    )
  },

  draw_key = draw_key_point
)

# Helper function for shape translation
translate_shape_string <- function(shape_string) {
  # Convert shape names to numeric codes
  shape_codes <- c(
    "circle" = 19,
    "square" = 15,
    "diamond" = 18,
    "triangle" = 17
  )

  unname(shape_codes[shape_string])
}

# Statistical transformation functions
stat_summary <- function(data, fun = mean, ...) {
  if (!is.data.frame(data)) {
    stop("Data must be a data frame")
  }

  summary_stats <- aggregate(data, by = list(data$group), FUN = fun, ...)
  return(summary_stats)
}

# Data manipulation utilities
filter_outliers <- function(x, threshold = 3) {
  mean_val <- mean(x, na.rm = TRUE)
  sd_val <- sd(x, na.rm = TRUE)

  z_scores <- abs((x - mean_val) / sd_val)
  x[z_scores <= threshold]
}
