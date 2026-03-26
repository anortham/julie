// Julie Dashboard - Client-side logic

// ---------------------------------------------------------------------------
// Theme management
// ---------------------------------------------------------------------------

(function initTheme() {
  const saved = localStorage.getItem("julie-theme") || "dark";
  document.documentElement.setAttribute("data-theme", saved);
})();

function toggleTheme() {
  const html = document.documentElement;
  const current = html.getAttribute("data-theme") || "dark";
  const next = current === "dark" ? "light" : "dark";
  html.setAttribute("data-theme", next);
  localStorage.setItem("julie-theme", next);
}

// ---------------------------------------------------------------------------
// Uptime formatting
// ---------------------------------------------------------------------------

/**
 * Format a duration in seconds into a human-readable string.
 * e.g. 3661 -> "1h 1m 1s"
 *
 * @param {number} seconds
 * @returns {string}
 */
function formatUptime(seconds) {
  if (seconds < 60) {
    return seconds + "s";
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) {
    return h + "h " + m + "m " + s + "s";
  }
  return m + "m " + s + "s";
}
