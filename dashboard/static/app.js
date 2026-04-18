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

function animateValue(obj, start, end, duration) {
  let startTimestamp = null;
  const step = (timestamp) => {
    if (!startTimestamp) startTimestamp = timestamp;
    const progress = Math.min((timestamp - startTimestamp) / duration, 1);
    // Ease out quart
    const easeOutIter = 1 - Math.pow(1 - progress, 4);
    obj.innerHTML = Math.floor(easeOutIter * (end - start) + start);
    if (progress < 1) {
      window.requestAnimationFrame(step);
    } else {
      obj.innerHTML = end;
    }
  };
  window.requestAnimationFrame(step);
}

// Tracks last-seen integer values per card label so htmx swaps animate from
// the previous value instead of snapping or counting up from 0 every refresh.
const previousValues = new Map();

function cardKey(valueEl) {
  const card = valueEl.closest('.julie-card');
  const label = card && card.querySelector('.label-text');
  return label ? label.innerText.trim() : null;
}

function animateTallies(root, fromZero) {
  const duration = fromZero ? 1500 : 600;
  root.querySelectorAll('.value-text').forEach(el => {
    const text = el.innerText.trim();
    if (!/^\d+$/.test(text)) return;
    const endVal = parseInt(text, 10);
    const key = cardKey(el);

    let startVal;
    if (fromZero) {
      startVal = 0;
    } else if (key && previousValues.has(key)) {
      startVal = previousValues.get(key);
    } else {
      startVal = endVal;
    }

    if (key) previousValues.set(key, endVal);

    if (startVal !== endVal) {
      el.innerText = String(startVal);
      animateValue(el, startVal, endVal, duration);
    }
  });
}

document.addEventListener("DOMContentLoaded", function() {
  animateTallies(document, true);
});

// Re-animate after htmx swaps so refreshed numbers still feel alive.
document.body.addEventListener('htmx:afterSwap', function(e) {
  animateTallies(e.detail.target, false);
});
