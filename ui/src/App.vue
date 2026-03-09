<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { RouterLink, RouterView } from 'vue-router'

const isDark = ref(false)

function toggleTheme() {
  isDark.value = !isDark.value
  document.documentElement.setAttribute('data-theme', isDark.value ? 'dark' : 'light')
  localStorage.setItem('julie-theme', isDark.value ? 'dark' : 'light')
}

onMounted(() => {
  const saved = localStorage.getItem('julie-theme')
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  isDark.value = saved ? saved === 'dark' : prefersDark
  document.documentElement.setAttribute('data-theme', isDark.value ? 'dark' : 'light')
})
</script>

<template>
  <div class="app-layout">
    <header class="app-header">
      <div class="header-brand">
        <span class="brand-icon pi pi-code"></span>
        <span class="brand-name">Julie</span>
      </div>
      <nav class="header-nav">
        <RouterLink to="/" class="nav-link">
          <span class="pi pi-home"></span>
          Dashboard
        </RouterLink>
        <RouterLink to="/projects" class="nav-link">
          <span class="pi pi-folder"></span>
          Projects
        </RouterLink>
        <RouterLink to="/search" class="nav-link">
          <span class="pi pi-search"></span>
          Search
        </RouterLink>
        <RouterLink to="/memories" class="nav-link">
          <span class="pi pi-clock"></span>
          Memories
        </RouterLink>
        <RouterLink to="/agents" class="nav-link">
          <span class="pi pi-bolt"></span>
          Agents
        </RouterLink>
      </nav>
      <button class="theme-toggle" @click="toggleTheme" :title="isDark ? 'Switch to light mode' : 'Switch to dark mode'">
        <span :class="isDark ? 'pi pi-sun' : 'pi pi-moon'"></span>
      </button>
    </header>
    <main class="app-main">
      <RouterView />
    </main>
  </div>
</template>

<style>
/* =========================================================================
   Design tokens — light (default) and dark
   ========================================================================= */

:root,
[data-theme="light"] {
  --app-bg: #f8f9fa;
  --card-bg: #ffffff;
  --input-bg: #ffffff;
  --header-bg: #1e293b;
  --header-text: #f1f5f9;

  --text-primary: #1e293b;
  --text-secondary: #64748b;
  --text-muted: #94a3b8;

  --brand-color: #818cf8;
  --color-primary: #6366f1;
  --color-primary-hover: #4f46e5;
  --color-primary-border: #c7d2fe;
  --color-primary-bg: rgba(99, 102, 241, 0.08);

  --color-success: #16a34a;
  --color-warning: #d97706;
  --color-error: #dc2626;
  --color-error-border: #fca5a5;
  --color-error-bg: rgba(220, 38, 38, 0.06);
  --color-warning-bg: #fffbeb;
  --color-warning-border: #fde68a;
  --color-info: #2563eb;
  --color-purple: #7c3aed;
  --color-purple-border: #d8b4fe;

  --border-color: #e2e8f0;
  --focus-ring: rgba(99, 102, 241, 0.2);
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.1);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.1);

  --code-bg: #f1f5f9;
  --hover-bg: rgba(0, 0, 0, 0.04);
}

[data-theme="dark"] {
  --app-bg: #0f172a;
  --card-bg: #1e293b;
  --input-bg: #1e293b;
  --header-bg: #020617;
  --header-text: #e2e8f0;

  --text-primary: #e2e8f0;
  --text-secondary: #94a3b8;
  --text-muted: #64748b;

  --brand-color: #818cf8;
  --color-primary: #818cf8;
  --color-primary-hover: #a5b4fc;
  --color-primary-border: #3730a3;
  --color-primary-bg: rgba(129, 140, 248, 0.12);

  --color-success: #4ade80;
  --color-warning: #fbbf24;
  --color-error: #f87171;
  --color-error-border: #7f1d1d;
  --color-error-bg: rgba(248, 113, 113, 0.1);
  --color-warning-bg: rgba(251, 191, 36, 0.1);
  --color-warning-border: #92400e;
  --color-info: #60a5fa;
  --color-purple: #a78bfa;
  --color-purple-border: #6d28d9;

  --border-color: #334155;
  --focus-ring: rgba(129, 140, 248, 0.3);
  --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.3);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.3);

  --code-bg: #0f172a;
  --hover-bg: rgba(255, 255, 255, 0.05);
}

/* =========================================================================
   Base styles
   ========================================================================= */

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: var(--app-bg);
  color: var(--text-primary);
}

/* =========================================================================
   Layout
   ========================================================================= */

.app-layout {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.app-header {
  background: var(--header-bg);
  color: var(--header-text);
  padding: 0 1.5rem;
  height: 56px;
  display: flex;
  align-items: center;
  gap: 2rem;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

.header-brand {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 1.25rem;
  font-weight: 700;
}

.brand-icon {
  color: var(--brand-color);
  font-size: 1.5rem;
}

.brand-name {
  color: var(--brand-color);
}

.header-nav {
  display: flex;
  gap: 0.25rem;
  flex: 1;
}

.nav-link {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 0.75rem;
  color: var(--header-text);
  text-decoration: none;
  border-radius: 6px;
  font-size: 0.875rem;
  transition: background 0.15s;
}

.nav-link:hover {
  background: rgba(255, 255, 255, 0.1);
}

.nav-link.router-link-active {
  background: rgba(129, 140, 248, 0.2);
  color: var(--brand-color);
}

.theme-toggle {
  background: none;
  border: 1px solid rgba(255, 255, 255, 0.15);
  color: var(--header-text);
  padding: 0.4rem 0.5rem;
  border-radius: 6px;
  cursor: pointer;
  font-size: 1rem;
  transition: background 0.15s, border-color 0.15s;
  display: flex;
  align-items: center;
}

.theme-toggle:hover {
  background: rgba(255, 255, 255, 0.1);
  border-color: rgba(255, 255, 255, 0.3);
}

.app-main {
  flex: 1;
  padding: 1.5rem;
  max-width: 1200px;
  width: 100%;
  margin: 0 auto;
}

/* =========================================================================
   Shared form styles (consumed by all views)
   ========================================================================= */

.form-input,
.form-select,
.form-textarea {
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 0.875rem;
  background: var(--input-bg);
  color: var(--text-primary);
  font-family: inherit;
}

.form-input:focus,
.form-select:focus,
.form-textarea:focus {
  outline: none;
  border-color: var(--color-primary);
  box-shadow: 0 0 0 2px var(--focus-ring);
}

.form-select {
  cursor: pointer;
  min-width: 120px;
}

.form-textarea {
  resize: vertical;
  min-height: 80px;
  width: 100%;
}

/* =========================================================================
   Shared button styles
   ========================================================================= */

.btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 1rem;
  border-radius: 6px;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  border: 1px solid transparent;
  transition: background 0.15s, border-color 0.15s;
}

.btn-primary {
  background: var(--color-primary);
  color: white;
  border-color: var(--color-primary);
}

.btn-primary:hover {
  background: var(--color-primary-hover);
  border-color: var(--color-primary-hover);
}

.btn-secondary {
  background: var(--card-bg);
  color: var(--text-primary);
  border-color: var(--border-color);
}

.btn-secondary:hover {
  background: var(--hover-bg);
}

.btn-text {
  background: none;
  color: var(--text-secondary);
  border: none;
  padding: 0.25rem 0.5rem;
}

.btn-text:hover {
  color: var(--text-primary);
}

.btn-danger {
  background: var(--color-error);
  color: white;
  border-color: var(--color-error);
}

/* =========================================================================
   Shared card styles
   ========================================================================= */

.card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1.25rem;
  box-shadow: var(--shadow-sm);
}

/* =========================================================================
   Shared status badge styles
   ========================================================================= */

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.2rem 0.6rem;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 500;
  border: 1px solid;
}

.badge-success {
  color: var(--color-success);
  border-color: var(--color-success);
  background: transparent;
}

.badge-warning {
  color: var(--color-warning);
  border-color: var(--color-warning-border);
  background: var(--color-warning-bg);
}

.badge-error {
  color: var(--color-error);
  border-color: var(--color-error-border);
  background: var(--color-error-bg);
}

.badge-info {
  color: var(--color-primary);
  border-color: var(--color-primary-border);
  background: var(--color-primary-bg);
}
</style>
