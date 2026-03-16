/* ============================================
   Julie Site — Script
   ============================================ */

(function () {
  'use strict';

  // --- Reduced motion check (live query — responds to runtime changes) ---
  const reducedMotionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
  function prefersReducedMotion() { return reducedMotionQuery.matches; }

  // --- Sticky nav show/hide ---
  const nav = document.getElementById('sticky-nav');
  const hero = document.getElementById('hero');

  if (nav && hero) {
    const observer = new IntersectionObserver(
      ([entry]) => {
        nav.classList.toggle('visible', !entry.isIntersecting);
      },
      { threshold: 0 }
    );
    observer.observe(hero);
  }

  // --- Active nav link tracking ---
  const navLinks = document.querySelectorAll('.nav-links a');
  const sections = document.querySelectorAll('.section, .hero');

  if (navLinks.length > 0 && sections.length > 0) {
    const sectionObserver = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            const id = entry.target.id;
            navLinks.forEach((link) => {
              link.classList.toggle('active', link.getAttribute('href') === '#' + id);
            });
          }
        });
      },
      { rootMargin: '-40% 0px -60% 0px' }
    );
    sections.forEach((s) => sectionObserver.observe(s));
  }

  // --- Scroll-triggered animation utility ---
  // Elements with class "animate-on-scroll" become visible when scrolled into view.
  // For staggered animations, add data-delay="100" (ms) on each element.
  function initScrollAnimations() {
    if (prefersReducedMotion()) {
      // Show everything immediately
      document.querySelectorAll('.animate-on-scroll').forEach((el) => {
        el.classList.add('visible');
      });
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            const delay = parseInt(entry.target.dataset.delay || '0', 10);
            setTimeout(() => entry.target.classList.add('visible'), delay);
            observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.1 }
    );

    document.querySelectorAll('.animate-on-scroll').forEach((el) => {
      observer.observe(el);
    });
  }

  // --- Hero animation ---
  function runHeroAnimation() {
    const bar = document.getElementById('context-bar-fill');
    const counter = document.getElementById('token-counter');
    const reads = document.getElementById('file-reads');
    const message = document.getElementById('hero-message');
    const ctas = document.getElementById('hero-ctas');

    if (!bar || !counter || !reads || !message || !ctas) return;

    // Hero animation runs once on page load (above fold, no Intersection Observer).
    // It's called exactly once by runHeroAnimation() below — no replay guard needed.

    // Skip animation for reduced motion
    if (prefersReducedMotion()) {
      bar.style.width = '5%';
      bar.className = 'context-bar-fill';
      counter.textContent = '~550 tokens';
      reads.innerHTML = [
        '<div class="file-read-line visible"><span class="julie-cmd">fast_search("UserService", definitions)</span> → <span class="julie-result">Found · 150 tokens</span></div>',
        '<div class="file-read-line visible"><span class="julie-cmd">deep_dive("UserService", overview)</span> → <span class="julie-result">Full picture · 400 tokens</span></div>',
      ].join('');
      message.textContent = 'Same understanding. 90% fewer tokens.';
      message.className = 'hero-message visible solution';
      ctas.classList.add('visible');
      var hint = document.querySelector('.scroll-hint');
      if (hint) hint.classList.add('visible');
      return;
    }

    const fileReads = [
      { file: 'src/services/user.rs', tokens: 2847 },
      { file: 'src/services/auth.rs', tokens: 1923 },
      { file: 'src/handlers/api.rs', tokens: 3104 },
      { file: 'src/models/user.rs', tokens: 1456 },
      { file: 'src/database/queries.rs', tokens: 2231 },
      { file: 'src/middleware/session.rs', tokens: 1889 },
    ];
    const totalCapacity = 16000;
    let currentTokens = 0;
    let lineIndex = 0;

    // Phase 1: The Problem
    function addFileRead() {
      if (lineIndex >= fileReads.length) {
        // Session over
        setTimeout(() => {
          message.textContent = 'Session over. You read 6 files.';
          message.className = 'hero-message visible problem';
          // Pause, then Phase 2
          setTimeout(startPhase2, 2000);
        }, 500);
        return;
      }

      const fr = fileReads[lineIndex];
      currentTokens += fr.tokens;
      const pct = Math.min((currentTokens / totalCapacity) * 100, 95);

      const line = document.createElement('div');
      line.className = 'file-read-line';
      line.innerHTML = '<span class="filename">Reading ' + fr.file + '...</span> <span class="tokens">' + fr.tokens.toLocaleString() + ' tokens</span>';
      reads.appendChild(line);
      requestAnimationFrame(() => line.classList.add('visible'));

      bar.style.width = pct + '%';
      if (pct > 70) bar.className = 'context-bar-fill danger';
      else if (pct > 40) bar.className = 'context-bar-fill warning';
      counter.textContent = currentTokens.toLocaleString() + ' tokens';

      lineIndex++;
      setTimeout(addFileRead, 600);
    }

    // Phase 2: With Julie
    function startPhase2() {
      reads.innerHTML = '';
      bar.style.width = '0%';
      bar.className = 'context-bar-fill';
      currentTokens = 0;
      counter.textContent = '0 tokens';
      message.className = 'hero-message';

      const julieSteps = [
        { cmd: 'fast_search("UserService", definitions)', result: 'Found · 150 tokens', tokens: 150 },
        { cmd: 'deep_dive("UserService", overview)', result: 'Callers, callees, types · 400 tokens', tokens: 400 },
      ];

      let stepIdx = 0;

      function addJulieStep() {
        if (stepIdx >= julieSteps.length) {
          setTimeout(() => {
            message.textContent = 'Same understanding. 90% fewer tokens.';
            message.className = 'hero-message visible solution';
            setTimeout(() => {
              ctas.classList.add('visible');
              // Show scroll hint after CTAs appear
              var hint = document.querySelector('.scroll-hint');
              if (hint) setTimeout(() => hint.classList.add('visible'), 400);
            }, 600);
          }, 500);
          return;
        }

        const s = julieSteps[stepIdx];
        currentTokens += s.tokens;
        const pct = (currentTokens / totalCapacity) * 100;

        const line = document.createElement('div');
        line.className = 'file-read-line';
        line.innerHTML = '<span class="julie-cmd">' + s.cmd + '</span> → <span class="julie-result">' + s.result + '</span>';
        reads.appendChild(line);
        requestAnimationFrame(() => line.classList.add('visible'));

        bar.style.width = pct + '%';
        counter.textContent = currentTokens.toLocaleString() + ' tokens';

        stepIdx++;
        setTimeout(addJulieStep, 1200);
      }

      setTimeout(addJulieStep, 500);
    }

    // Start Phase 1 after a brief delay
    setTimeout(addFileRead, 800);
  }

  runHeroAnimation();

  // --- Reference graph entrance animation ---
  function initGraphAnimation() {
    const graph = document.getElementById('ref-graph');
    if (!graph) return;

    if (prefersReducedMotion()) {
      // Edges and nodes stay visible (CSS defaults)
      return;
    }

    // Mark SVG as animation-ready (hides nodes/edges via CSS)
    graph.classList.add('graph-animate-ready');

    // Pre-compute edge lengths and set dasharray/offset from JS
    const edges = graph.querySelectorAll('.graph-edge-animated');
    edges.forEach((edge) => {
      const length = edge.getTotalLength();
      edge.style.strokeDasharray = length;
      edge.style.strokeDashoffset = length;
    });

    const nodes = graph.querySelectorAll('.graph-node-animated');

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.unobserve(entry.target);

        // Nodes appear first, staggered
        nodes.forEach((node, i) => {
          setTimeout(() => node.classList.add('shown'), i * 120);
        });

        // Edges draw after nodes are visible
        const edgeDelay = nodes.length * 120 + 200;
        edges.forEach((edge, i) => {
          setTimeout(() => edge.classList.add('drawn'), edgeDelay + i * 100);
        });
      },
      { threshold: 0.3 }
    );

    observer.observe(graph);
  }

  initGraphAnimation();

  // --- Performance number count-up ---
  function initPerfCountUp() {
    const perfNumbers = document.querySelectorAll('.perf-number[data-target]');
    if (perfNumbers.length === 0) return;

    if (prefersReducedMotion()) return; // Numbers already show final values in HTML

    // Zero out for animation start (reduced-motion users keep the HTML values above)
    perfNumbers.forEach((el) => {
      const prefix = el.dataset.prefix || '';
      const suffix = el.dataset.suffix || '';
      el.textContent = prefix + '0' + suffix;
    });

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          observer.unobserve(entry.target);

          const el = entry.target;
          const target = parseInt(el.dataset.target, 10);
          const prefix = el.dataset.prefix || '';
          const suffix = el.dataset.suffix || '';
          const duration = 1000;
          const start = performance.now();

          function update(now) {
            const elapsed = now - start;
            const progress = Math.min(elapsed / duration, 1);
            // Ease out
            const eased = 1 - Math.pow(1 - progress, 3);
            const current = Math.round(eased * target);
            el.textContent = prefix + current + suffix;
            if (progress < 1) requestAnimationFrame(update);
          }

          requestAnimationFrame(update);
        });
      },
      { threshold: 0.5 }
    );

    perfNumbers.forEach((el) => observer.observe(el));
  }

  initPerfCountUp();

  // --- Installation tab switching ---
  const tabs = document.querySelectorAll('.install-tab');
  const panels = document.querySelectorAll('.install-panel');
  const tabList = [...tabs];

  function activateTab(tab) {
    const target = tab.dataset.tab;
    tabs.forEach((t) => {
      const isActive = t === tab;
      t.classList.toggle('active', isActive);
      t.setAttribute('aria-selected', isActive ? 'true' : 'false');
      t.setAttribute('tabindex', isActive ? '0' : '-1');
    });
    panels.forEach((p) => {
      p.classList.toggle('active', p.id === 'panel-' + target);
    });
  }

  tabs.forEach((tab) => {
    tab.addEventListener('click', () => activateTab(tab));

    // Keyboard navigation: Arrow Left/Right, Home/End
    tab.addEventListener('keydown', (e) => {
      const idx = tabList.indexOf(tab);
      let next = null;
      if (e.key === 'ArrowRight') next = tabList[(idx + 1) % tabList.length];
      else if (e.key === 'ArrowLeft') next = tabList[(idx - 1 + tabList.length) % tabList.length];
      else if (e.key === 'Home') next = tabList[0];
      else if (e.key === 'End') next = tabList[tabList.length - 1];
      if (next) {
        e.preventDefault();
        next.focus();
        activateTab(next);
      }
    });
  });

  // Set initial tabindex
  tabs.forEach((t) => t.setAttribute('tabindex', t.classList.contains('active') ? '0' : '-1'));

  // --- Copy to clipboard ---
  const copySnippets = {
    'claude-code': 'git clone https://github.com/anortham/julie.git\ncd julie && cargo build --release\nclaude mcp add julie -- ./target/release/julie-server',
    'vscode': '{\n  "servers": {\n    "Julie": {\n      "type": "stdio",\n      "command": "/path/to/julie-server",\n      "env": {\n        "JULIE_WORKSPACE": "${workspaceFolder}"\n      }\n    }\n  }\n}',
    'opencode': '{\n  "mcp": {\n    "julie": {\n      "type": "local",\n      "command": ["/path/to/julie-server"],\n      "enabled": true\n    }\n  }\n}',
    'cursor': '{\n  "mcpServers": {\n    "julie": {\n      "command": "/path/to/julie-server"\n    }\n  }\n}',
  };

  document.querySelectorAll('.copy-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const key = btn.dataset.copy;
      const text = copySnippets[key];
      if (!text) return;

      navigator.clipboard.writeText(text).then(() => {
        btn.textContent = 'Copied!';
        btn.classList.add('copied');
        setTimeout(() => {
          btn.textContent = 'Copy';
          btn.classList.remove('copied');
        }, 2000);
      }).catch(() => {
        // Fallback for non-secure contexts (file:// protocol, etc.)
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.opacity = '0';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
        btn.textContent = 'Copied!';
        btn.classList.add('copied');
        setTimeout(() => {
          btn.textContent = 'Copy';
          btn.classList.remove('copied');
        }, 2000);
      });
    });
  });

  // --- Hide scroll hint on first scroll ---
  var scrollHint = document.querySelector('.scroll-hint');
  if (scrollHint) {
    window.addEventListener('scroll', function hideHint() {
      scrollHint.style.opacity = '0';
      window.removeEventListener('scroll', hideHint);
    }, { passive: true });
  }

  // Run after DOM is ready (script is at bottom of body, so DOM is ready)
  initScrollAnimations();
})();
