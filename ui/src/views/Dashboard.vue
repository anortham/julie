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

interface AgentStats {
  total_dispatches: number
  last_dispatch: string | null
}

interface BackendInfo {
  name: string
  available: boolean
  version?: string
}

interface EmbeddingProjectStatus {
  project: string
  workspace_id: string
  backend: string | null
  accelerated: boolean | null
  degraded_reason: string | null
  embedding_count: number
  initialized: boolean
}

interface DashboardStats {
  projects: ProjectStats
  agents: AgentStats
  backends: BackendInfo[]
  embeddings: EmbeddingProjectStatus[]
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const health = ref<HealthData | null>(null)
const stats = ref<DashboardStats | null>(null)
const error = ref<string | null>(null)
const statsError = ref<string | null>(null)
const loading = ref(true)
const checkingEmbeddings = ref(false)
const exportingDiagnostics = ref(false)

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

async function checkEmbeddings() {
  checkingEmbeddings.value = true
  try {
    const res = await fetch('/api/embeddings/check', { method: 'POST' })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const updated: EmbeddingProjectStatus[] = await res.json()
    if (stats.value) {
      stats.value = { ...stats.value, embeddings: updated }
    }
  } catch (e) {
    // Non-fatal — the card will just keep showing current state
  } finally {
    checkingEmbeddings.value = false
  }
}

async function exportDiagnostics() {
  exportingDiagnostics.value = true
  try {
    const res = await fetch('/api/diagnostics/report')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const data = await res.json()
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    const ts = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
    a.download = `julie-diagnostic-${ts}.json`
    a.click()
    URL.revokeObjectURL(url)
  } catch (e) {
    // Could show a toast, but for now just log it
    console.error('Failed to export diagnostics:', e)
  } finally {
    exportingDiagnostics.value = false
  }
}

function embeddingDotClass(e: EmbeddingProjectStatus): string {
  if (!e.initialized) return 'dot-uninit'
  if (e.accelerated) return 'dot-available'
  if (e.degraded_reason) return 'dot-degraded'
  return 'dot-available'
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

        <!-- Embeddings card -->
        <div class="card">
          <div class="card-header">
            <span class="pi pi-microchip card-icon"></span>
            <span class="card-label">Embeddings</span>
          </div>
          <div v-if="stats.embeddings.length === 0" class="card-value card-value-sm">No projects</div>
          <div v-else class="embeddings-list">
            <div
              v-for="e in stats.embeddings"
              :key="e.workspace_id"
              class="embedding-row"
            >
              <span class="embedding-dot" :class="embeddingDotClass(e)"></span>
              <span class="embedding-name">{{ e.project }}</span>
              <span v-if="e.initialized && e.backend" class="embedding-backend">
                {{ e.backend }}
                <span v-if="e.accelerated" class="accel-bolt" title="GPU accelerated">&#9889;</span>
              </span>
              <span v-else class="embedding-backend embedding-uninit">not initialized</span>
              <span class="embedding-count">{{ e.embedding_count.toLocaleString() }}</span>
            </div>
            <div
              v-for="e in stats.embeddings.filter(x => x.degraded_reason)"
              :key="'deg-' + e.workspace_id"
              class="embedding-degraded"
            >
              <span class="pi pi-exclamation-triangle"></span>
              {{ e.project }}: {{ e.degraded_reason }}
            </div>
          </div>
          <button
            class="check-btn"
            :disabled="checkingEmbeddings"
            @click="checkEmbeddings"
          >
            <span :class="checkingEmbeddings ? 'pi pi-spin pi-spinner' : 'pi pi-refresh'"></span>
            {{ checkingEmbeddings ? 'Checking...' : 'Check Status' }}
          </button>
        </div>

        <!-- Diagnostics card -->
        <div class="card">
          <div class="card-header">
            <span class="pi pi-file-export card-icon"></span>
            <span class="card-label">Diagnostics</span>
          </div>
          <div class="card-value card-value-sm">Export logs, system info, and project state for debugging.</div>
          <button
            class="check-btn"
            :disabled="exportingDiagnostics"
            @click="exportDiagnostics"
          >
            <span :class="exportingDiagnostics ? 'pi pi-spin pi-spinner' : 'pi pi-download'"></span>
            {{ exportingDiagnostics ? 'Exporting...' : 'Export Diagnostic Report' }}
          </button>
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

/* Embeddings list */
.embeddings-list {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  margin-top: 0.25rem;
}

.embedding-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.85rem;
}

.embedding-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-uninit {
  background: var(--text-muted);
}

.dot-degraded {
  background: var(--color-warning);
}

.embedding-name {
  font-weight: 600;
  color: var(--text-primary);
}

.embedding-backend {
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.embedding-uninit {
  font-style: italic;
  color: var(--text-muted);
}

.accel-bolt {
  font-size: 0.65rem;
}

.embedding-count {
  margin-left: auto;
  font-size: 0.75rem;
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.embedding-degraded {
  font-size: 0.7rem;
  color: var(--color-warning);
  display: flex;
  align-items: center;
  gap: 0.3rem;
  padding-top: 0.2rem;
}

.embedding-degraded .pi {
  font-size: 0.6rem;
}

.check-btn {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  margin-top: 0.6rem;
  padding: 0.3rem 0.6rem;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 0.75rem;
  cursor: pointer;
  transition: all 0.15s;
  width: 100%;
  justify-content: center;
}

.check-btn:hover:not(:disabled) {
  background: var(--color-primary-bg);
  color: var(--color-primary);
  border-color: var(--color-primary-border);
}

.check-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.check-btn .pi {
  font-size: 0.7rem;
}
</style>
