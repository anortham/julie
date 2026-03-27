// Julie Dashboard - Client-side logic

// ---------------------------------------------------------------------------
// Theme management
// ---------------------------------------------------------------------------

(function initTheme() {
  var saved = localStorage.getItem("julie-theme") || "dark";
  document.documentElement.setAttribute("data-theme", saved);
  // Update icon once DOM is ready
  document.addEventListener("DOMContentLoaded", function() {
    updateThemeIcon(saved);
  });
})();

function toggleTheme() {
  var html = document.documentElement;
  var current = html.getAttribute("data-theme") || "dark";
  var next = current === "dark" ? "light" : "dark";
  html.setAttribute("data-theme", next);
  localStorage.setItem("julie-theme", next);
  updateThemeIcon(next);
}

function updateThemeIcon(theme) {
  var icon = document.getElementById("theme-icon");
  if (icon) {
    // Moon for dark, sun for light
    icon.textContent = theme === "dark" ? "\u263E" : "\u2600";
  }
}
