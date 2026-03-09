<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { RouterLink } from 'vue-router'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface HealthData {
  status: string
  version: string
  uptime_seconds: number
}

interface ProjectStats {
  total: number
  ready: number
  indexing: number
  error: number
  registered: number
  stale: number
}

interface MemoryStats {
  total_checkpoints: number
  active_plan: string | null
  last_checkpoint: string | null
}

interface AgentStats {
  total_dispatches: number
  last_dispatch: string | null
}

interface BackendInfo {
  name: string
  available: boolean
  version?: string
}

interface DashboardStats {
  projects: ProjectStats
  memories: MemoryStats
  agents: AgentStats
  backends: BackendInfo[]
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const health = ref<HealthData | null>(null)
const stats = ref<DashboardStats | null>(null)
const error = ref<string | null>(null)
const statsError = ref<string | null>(null)
const loading = ref(true)

// ---------------------------------------------------------------------------
// Computed
// ---------------------------------------------------------------------------

const uptimeFormatted = computed(() => {
  if (!health.value) return '--'
  const s = health.value.uptime_seconds
  const days = Math.floor(s / 86400)
  const hours = Math.floor((s % 86400) / 3600)
  const minutes = Math.floor((s % 3600) / 60)
  const secs = s % 60
  const parts: string[] = []
  if (days > 0) parts.push(`${days}d`)
  if (hours > 0) parts.push(`${hours}h`)
  if (minutes > 0) parts.push(`${minutes}m`)
  parts.push(`${secs}s`)
  return parts.join(' ')
})

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function relativeTime(iso: string | null): string {
  if (!iso) return '--'
  const date = new Date(iso)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffSec = Math.floor(diffMs / 1000)
  const diffMin = Math.floor(diffSec / 60)
  const diffHour = Math.floor(diffMin / 60)
  const diffDay = Math.floor(diffHour / 24)

  if (diffSec < 60) return 'just now'
  if (diffMin < 60) return `${diffMin}m ago`
  if (diffHour < 24) return `${diffHour}h ago`
  if (diffDay < 7) return `${diffDay}d ago`
  if (diffDay < 30) return `${Math.floor(diffDay / 7)}w ago`
  return date.toLocaleDateString()
}

// ---------------------------------------------------------------------------
// Fetch
// ---------------------------------------------------------------------------

async function fetchHealth() {
  try {
    const res = await fetch('/api/health')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    health.value = await res.json()
    error.value = null
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch health'
  }
}

async function fetchStats() {
  try {
    const res = await fetch('/api/dashboard/stats')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    stats.value = await res.json()
    statsError.value = null
  } catch (e) {
    statsError.value = e instanceof Error ? e.message : 'Failed to fetch stats'
  }
}

onMounted(async () => {
  await Promise.all([fetchHealth(), fetchStats()])
  loading.value = false
})
</script>

<template>
  <div class="dashboard">
    <h1 class="page-title">Dashboard</h1>

    <div v-if="loading" class="status-message">
      <span class="pi pi-spin pi-spinner"></span> Loading...
    </div>

    <div v-else-if="error" class="status-message status-error">
      <span class="pi pi-exclamation-triangle"></span>
      Failed to connect: {{ error }}
    </div>

    <template v-else>
      <!-- Row 1: Health cards (existing) -->
      <div class="cards">
        <div class="card">
          <div class="card-header">
            <span class="pi pi-heart card-icon status-ok"></span>
            <span class="card-label">Status</span>
          </div>
          <div class="card-value status-ok">{{ health?.status ?? '--' }}</div>
        </div>

        <div class="card">
          <div class="card-header">
            <span class="pi pi-tag card-icon"></span>
            <span class="card-label">Version</span>
          </div>
          <div class="card-value">v{{ health?.version ?? '--' }}</div>
        </div>

        <div class="card">
          <div class="card-header">
            <span class="pi pi-clock card-icon"></span>
            <span class="card-label">Uptime</span>
          </div>
          <div class="card-value">{{ uptimeFormatted }}</div>
        </div>
      </div>

      <!-- Row 2: Stats cards -->
      <div v-if="stats" class="cards cards-row2">
        <!-- Projects card -->
        <RouterLink to="/projects" class="card card-link">
          <div class="card-header">
            <span class="pi pi-folder card-icon"></span>
            <span class="card-label">Projects</span>
            <span class="pi pi-arrow-right card-arrow"></span>
          </div>
          <div class="card-value">{{ stats.projects.total }}</div>
          <div class="card-breakdown">
            <span v-if="stats.projects.ready > 0" class="breakdown-item breakdown-ready">
              <span class="breakdown-dot dot-ready"></span>
              {{ stats.projects.ready }} ready
            </span>
            <span v-if="stats.projects.indexing > 0" class="breakdown-item breakdown-indexing">
              <span class="breakdown-dot dot-indexing"></span>
              {{ stats.projects.indexing }} indexing
            </span>
            <span v-if="stats.projects.error > 0" class="breakdown-item breakdown-error">
              <span class="breakdown-dot dot-error"></span>
              {{ stats.projects.error }} error
            </span>
            <span v-if="stats.projects.registered > 0" class="breakdown-item">
              <span class="breakdown-dot dot-registered"></span>
              {{ stats.projects.registered }} registered
            </span>
            <span v-if="stats.projects.stale > 0" class="breakdown-item">
              <span class="breakdown-dot dot-stale"></span>
              {{ stats.projects.stale }} stale
            </span>
            <span v-if="stats.projects.total === 0" class="breakdown-empty">
              No projects registered
            </span>
          </div>
        </RouterLink>

        <!-- Memory card -->
        <RouterLink to="/memories" class="card card-link">
          <div class="card-header">
            <span class="pi pi-clock card-icon"></span>
            <span class="card-label">Memories</span>
            <span class="pi pi-arrow-right card-arrow"></span>
          </div>
          <div class="card-value">{{ stats.memories.total_checkpoints }}</div>
          <div class="card-breakdown">
            <span class="breakdown-label">checkpoints</span>
          </div>
          <div class="card-detail">
            <div v-if="stats.memories.active_plan" class="detail-row">
              <span class="pi pi-map detail-icon"></span>
              <span class="detail-text">{{ stats.memories.active_plan }}</span>
            </div>
            <div v-if="stats.memories.last_checkpoint" class="detail-row">
              <span class="pi pi-calendar detail-icon"></span>
              <span class="detail-text detail-muted">Last {{ relativeTime(stats.memories.last_checkpoint) }}</span>
            </div>
            <div v-if="stats.memories.total_checkpoints === 0 && !stats.memories.active_plan" class="breakdown-empty">
              No checkpoints yet
            </div>
          </div>
        </RouterLink>

        <!-- Agents card -->
        <RouterLink to="/agents" class="card card-link">
          <div class="card-header">
            <span class="pi pi-bolt card-icon"></span>
            <span class="card-label">Agents</span>
            <span class="pi pi-arrow-right card-arrow"></span>
          </div>
          <div class="card-value">{{ stats.agents.total_dispatches }}</div>
          <div class="card-breakdown">
            <span class="breakdown-label">dispatches</span>
          </div>
          <div class="card-detail">
            <div v-if="stats.agents.last_dispatch" class="detail-row">
              <span class="pi pi-calendar detail-icon"></span>
              <span class="detail-text detail-muted">Last {{ relativeTime(stats.agents.last_dispatch) }}</span>
            </div>
            <div v-if="stats.agents.total_dispatches === 0" class="breakdown-empty">
              No dispatches yet
            </div>
          </div>
        </RouterLink>

        <!-- Backends card -->
        <div class="card">
          <div class="card-header">
            <span class="pi pi-server card-icon"></span>
            <span class="card-label">Backends</span>
          </div>
          <div v-if="stats.backends.length === 0" class="card-value card-value-sm">None detected</div>
          <div v-else class="backends-list">
            <div
              v-for="b in stats.backends"
              :key="b.name"
              class="backend-row"
            >
              <span
                class="backend-dot"
                :class="b.available ? 'dot-available' : 'dot-unavailable'"
              ></span>
              <span class="backend-name">{{ b.name }}</span>
              <span v-if="b.version" class="backend-version">v{{ b.version }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Stats fetch error (non-fatal, health still shows) -->
      <div v-if="statsError" class="status-message status-warn">
        <span class="pi pi-info-circle"></span>
        Could not load dashboard stats: {{ statsError }}
      </div>
    </template>
  </div>
</template>

<style scoped>
.page-title {
  font-size: 1.5rem;
  font-weight: 600;
  margin-bottom: 1.25rem;
}

.status-message {
  padding: 1rem;
  border-radius: 8px;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: var(--text-secondary);
}

.status-error {
  border-color: var(--color-error-border);
  color: var(--color-error);
  background: var(--color-error-bg);
}

.status-warn {
  border-color: var(--color-warning-border);
  color: var(--color-warning);
  background: var(--color-warning-bg);
  margin-top: 1rem;
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 1rem;
}

.cards-row2 {
  margin-top: 1rem;
}

.card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1.25rem;
}

.card-link {
  text-decoration: none;
  color: inherit;
  transition: border-color 0.15s, box-shadow 0.15s;
  cursor: pointer;
}

.card-link:hover {
  border-color: var(--color-primary-border);
  box-shadow: 0 1px 4px var(--focus-ring);
}

.card-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
}

.card-icon {
  font-size: 1.1rem;
  color: var(--text-secondary);
}

.card-label {
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
}

.card-arrow {
  margin-left: auto;
  font-size: 0.75rem;
  color: var(--text-secondary);
  opacity: 0;
  transition: opacity 0.15s;
}

.card-link:hover .card-arrow {
  opacity: 1;
}

.card-value {
  font-size: 1.5rem;
  font-weight: 700;
}

.card-value-sm {
  font-size: 1rem;
  color: var(--text-secondary);
}

.status-ok {
  color: var(--color-success);
}

/* Breakdown items (project status indicators) */
.card-breakdown {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.6rem;
  margin-top: 0.5rem;
}

.breakdown-item {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 0.75rem;
  color: var(--text-secondary);
  font-weight: 500;
}

.breakdown-label {
  font-size: 0.75rem;
  color: var(--text-secondary);
  font-weight: 500;
}

.breakdown-empty {
  font-size: 0.75rem;
  color: var(--text-secondary);
  font-style: italic;
}

.breakdown-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-ready {
  background: var(--color-success);
}

.dot-indexing {
  background: var(--color-warning);
}

.dot-error {
  background: var(--color-error);
}

.dot-registered {
  background: var(--color-primary);
}

.dot-stale {
  background: var(--text-muted);
}

/* Card detail rows (memory/agent extra info) */
.card-detail {
  margin-top: 0.6rem;
  padding-top: 0.6rem;
  border-top: 1px solid var(--border-color);
}

.detail-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.8rem;
  margin-bottom: 0.3rem;
}

.detail-row:last-child {
  margin-bottom: 0;
}

.detail-icon {
  font-size: 0.7rem;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.detail-text {
  color: var(--text-primary);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-muted {
  color: var(--text-secondary);
  font-weight: 400;
}

/* Backends list */
.backends-list {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  margin-top: 0.25rem;
}

.backend-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.85rem;
}

.backend-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-available {
  background: var(--color-success);
}

.dot-unavailable {
  background: var(--color-error);
}

.backend-name {
  font-weight: 600;
  color: var(--text-primary);
}

.backend-version {
  font-size: 0.75rem;
  color: var(--text-secondary);
}
</style>
