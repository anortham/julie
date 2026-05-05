helper <- function(value) {
  value + 1
}

run_worker <- function(id) {
  helper(id)
}

Worker <- R6::R6Class(
  "Worker",
  public = list(
    id = NULL,
    initialize = function(id) {
      self$id <- id
    },
    run = function() {
      helper(self$id)
    }
  )
)
