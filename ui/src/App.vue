<script setup lang="ts">
import { RouterLink, RouterView } from 'vue-router'
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
      </nav>
    </header>
    <main class="app-main">
      <RouterView />
    </main>
  </div>
</template>

<style>
:root {
  --app-bg: #f8f9fa;
  --header-bg: #1e293b;
  --header-text: #f1f5f9;
  --brand-color: #818cf8;
  --card-bg: #ffffff;
  --text-primary: #1e293b;
  --text-secondary: #64748b;
  --border-color: #e2e8f0;
}

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

.app-main {
  flex: 1;
  padding: 1.5rem;
  max-width: 1200px;
  width: 100%;
  margin: 0 auto;
}
</style>
