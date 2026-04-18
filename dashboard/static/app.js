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

// ---------------------------------------------------------------------------
// Premium Form Flourishes & Animations
// ---------------------------------------------------------------------------

// WeakMap keyed by element stores in-flight rAF handles so animations can be
// cancelled before htmx replaces nodes or before a second animation fires on
// the same element.
const rafHandles = new WeakMap();

function animateValue(element, start, end, duration) {
  if (rafHandles.has(element)) {
    cancelAnimationFrame(rafHandles.get(element));
  }
  let startTimestamp = null;
  const step = (timestamp) => {
    if (!startTimestamp) startTimestamp = timestamp;
    const progress = Math.min((timestamp - startTimestamp) / duration, 1);
    // Ease out quart
    const easeOutIter = 1 - Math.pow(1 - progress, 4);
    element.textContent = String(Math.floor(easeOutIter * (end - start) + start));
    if (progress < 1) {
      rafHandles.set(element, requestAnimationFrame(step));
    } else {
      element.textContent = String(end);
      rafHandles.delete(element);
    }
  };
  rafHandles.set(element, requestAnimationFrame(step));
}

// Tracks last-seen integer values per card so htmx swaps animate from the
// previous value instead of snapping or counting up from 0 every refresh.
const previousValues = new Map();

// Key = scopeId + row-index + column-index + label so two cards that share
// the same label text at different positions never collide in previousValues.
function cardKey(valueEl, root) {
  const card = valueEl.closest('.julie-card');
  const label = card && card.querySelector('.label-text');
  if (!label) return null;
  const labelText = label.innerText.trim();
  const scopeId = (root && root.id) ? root.id : '';
  const col = card.closest('.column');
  const columns = col && col.closest('.columns');
  const colIdx = (col && columns) ? Array.from(columns.children).indexOf(col) : -1;
  const rowIdx = columns ? Array.from(document.querySelectorAll('.columns')).indexOf(columns) : -1;
  return `${scopeId}:${rowIdx}:${colIdx}:${labelText}`;
}

function animateTallies(root, fromZero) {
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const duration = fromZero ? 1500 : 600;
  root.querySelectorAll('.value-text').forEach(el => {
    const text = el.innerText.trim();
    if (!/^\d+$/.test(text)) return;
    const endVal = parseInt(text, 10);
    const key = cardKey(el, root);

    let startVal;
    if (fromZero) {
      startVal = 0;
    } else if (key && previousValues.has(key)) {
      startVal = previousValues.get(key);
    } else {
      startVal = endVal;
    }

    if (key) previousValues.set(key, endVal);

    if (reducedMotion) {
      el.textContent = String(endVal);
      return;
    }

    if (startVal !== endVal) {
      el.textContent = String(startVal);
      animateValue(el, startVal, endVal, duration);
    }
  });
}

document.addEventListener("DOMContentLoaded", function() {
  animateTallies(document, true);
});

// Cancel in-flight rAF callbacks before htmx replaces nodes so animations
// do not continue running on detached DOM elements.
document.body.addEventListener('htmx:beforeSwap', function(e) {
  e.detail.target.querySelectorAll('.value-text').forEach(el => {
    if (rafHandles.has(el)) {
      cancelAnimationFrame(rafHandles.get(el));
      rafHandles.delete(el);
    }
  });
});

// Re-animate after htmx swaps so refreshed numbers still feel alive.
document.body.addEventListener('htmx:afterSwap', function(e) {
  animateTallies(e.detail.target, false);
});
