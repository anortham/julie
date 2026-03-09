<script setup lang="ts">
import { ref, onMounted } from 'vue'

interface EmbeddingStatus {
  backend: string
  accelerated: boolean
  degraded_reason?: string
}

interface Project {
  workspace_id: string
  name: string
  path: string
  status: string
  last_indexed: string | null
  symbol_count: number | null
  file_count: number | null
  embedding_status: EmbeddingStatus | null
}

interface LanguageCount {
  language: string
  file_count: number
}

interface SymbolKindCount {
  kind: string
  count: number
}

interface ProjectStats {
  total_symbols: number
  total_files: number
  total_relationships: number
  db_size_mb: number
  embedding_count: number
  languages: LanguageCount[]
  symbol_kinds: SymbolKindCount[]
}

const projects = ref<Project[]>([])
const error = ref<string | null>(null)
const loading = ref(true)

// Register form state
const showRegister = ref(false)
const registerPath = ref('')
const registerError = ref<string | null>(null)
const registering = ref(false)

// Expandable stats
const expandedId = ref<string | null>(null)
const statsCache = ref<Record<string, ProjectStats>>({})
const statsLoading = ref<Record<string, boolean>>({})
const statsError = ref<Record<string, string | null>>({})

// Language colors — deterministic palette
const langColors = [
  '#6366f1', '#ec4899', '#f59e0b', '#10b981', '#3b82f6',
  '#8b5cf6', '#ef4444', '#14b8a6', '#f97316', '#06b6d4',
  '#84cc16', '#e879f9', '#fb923c', '#22d3ee', '#a3e635',
]

function langColor(index: number): string {
  return langColors[index % langColors.length]
}

function statusClass(status: string): string {
  if (status === 'ready') return 'badge-ready'
  if (status === 'indexing') return 'badge-indexing'
  if (status === 'registered') return 'badge-registered'
  if (status.startsWith('error')) return 'badge-error'
  return 'badge-default'
}

async function fetchProjects() {
  try {
    const res = await fetch('/api/projects')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    projects.value = await res.json()
    error.value = null
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch projects'
  } finally {
    loading.value = false
  }
}

async function registerProject() {
  if (!registerPath.value.trim()) return
  registering.value = true
  registerError.value = null
  try {
    const res = await fetch('/api/projects', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: registerPath.value.trim() }),
    })
    if (!res.ok && res.status !== 409) {
      const text = await res.text()
      throw new Error(text || `HTTP ${res.status}`)
    }
    registerPath.value = ''
    showRegister.value = false
    await fetchProjects()
  } catch (e) {
    registerError.value = e instanceof Error ? e.message : 'Failed to register'
  } finally {
    registering.value = false
  }
}

async function toggleStats(p: Project) {
  if (expandedId.value === p.workspace_id) {
    expandedId.value = null
    return
  }
  expandedId.value = p.workspace_id

  // Already cached
  if (statsCache.value[p.workspace_id]) return

  // Fetch on demand
  statsLoading.value[p.workspace_id] = true
  statsError.value[p.workspace_id] = null
  try {
    const res = await fetch(`/api/projects/${p.workspace_id}/stats`)
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    statsCache.value[p.workspace_id] = await res.json()
  } catch (e) {
    statsError.value[p.workspace_id] = e instanceof Error ? e.message : 'Failed to load stats'
  } finally {
    statsLoading.value[p.workspace_id] = false
  }
}

function formatKind(kind: string): string {
  return kind.replace(/_/g, ' ')
}

function timeAgo(iso: string): string {
  const now = Date.now()
  const then = new Date(iso).getTime()
  const diff = now - then
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  const months = Math.floor(days / 30)
  return `${months}mo ago`
}

function pluralFiles(n: number): string {
  return n === 1 ? '1 file' : `${n} files`
}

// Quick-launch state
const editorCommand = ref(localStorage.getItem('julie-editor-command') || 'code')
const showEditorConfig = ref(false)
const copiedPath = ref<string | null>(null)

function saveEditorCommand() {
  localStorage.setItem('julie-editor-command', editorCommand.value)
  showEditorConfig.value = false
}

async function copyPath(p: Project, event: Event) {
  event.stopPropagation()
  try {
    await navigator.clipboard.writeText(p.path)
    copiedPath.value = p.workspace_id
    setTimeout(() => { copiedPath.value = null }, 1500)
  } catch {
    // Fallback for non-HTTPS contexts
    const input = document.createElement('input')
    input.value = p.path
    document.body.appendChild(input)
    input.select()
    document.execCommand('copy')
    document.body.removeChild(input)
    copiedPath.value = p.workspace_id
    setTimeout(() => { copiedPath.value = null }, 1500)
  }
}

async function openInEditor(p: Project, event: Event) {
  event.stopPropagation()
  try {
    const res = await fetch('/api/launch/editor', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ editor: editorCommand.value, path: p.path }),
    })
    if (!res.ok) {
      const text = await res.text()
      alert(`Failed to open editor: ${text}`)
    }
  } catch (e) {
    alert(`Failed to open editor: ${e instanceof Error ? e.message : e}`)
  }
}

async function openInTerminal(p: Project, event: Event) {
  event.stopPropagation()
  try {
    const res = await fetch('/api/launch/terminal', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: p.path }),
    })
    if (!res.ok) {
      const text = await res.text()
      alert(`Failed to open terminal: ${text}`)
    }
  } catch (e) {
    alert(`Failed to open terminal: ${e instanceof Error ? e.message : e}`)
  }
}

onMounted(() => {
  fetchProjects()
})
</script>

<template>
  <div class="projects">
    <div class="page-header">
      <h1 class="page-title">Projects</h1>
      <button class="btn btn-primary" @click="showRegister = !showRegister">
        <span class="pi pi-plus"></span>
        Register Project
      </button>
    </div>

    <!-- Register form -->
    <div v-if="showRegister" class="register-form">
      <div class="form-row">
        <input
          v-model="registerPath"
          type="text"
          placeholder="Absolute path to project directory"
          class="form-input"
          @keyup.enter="registerProject"
        />
        <button
          class="btn btn-primary"
          :disabled="registering || !registerPath.trim()"
          @click="registerProject"
        >
          {{ registering ? 'Registering...' : 'Add' }}
        </button>
        <button class="btn btn-secondary" @click="showRegister = false">
          Cancel
        </button>
      </div>
      <div v-if="registerError" class="form-error">
        <span class="pi pi-exclamation-triangle"></span>
        {{ registerError }}
      </div>
    </div>

    <!-- Loading / Error -->
    <div v-if="loading" class="status-message">
      <span class="pi pi-spin pi-spinner"></span> Loading...
    </div>

    <div v-else-if="error" class="status-message status-error">
      <span class="pi pi-exclamation-triangle"></span>
      Failed to load projects: {{ error }}
    </div>

    <!-- Empty state -->
    <div v-else-if="projects.length === 0" class="empty-state">
      <span class="pi pi-folder-open empty-icon"></span>
      <p>No projects registered yet.</p>
      <p class="empty-hint">Click "Register Project" to add a codebase.</p>
    </div>

    <!-- Projects table -->
    <div v-else class="table-wrapper">
      <table class="projects-table">
        <thead>
          <tr>
            <th></th>
            <th>Name</th>
            <th>Path</th>
            <th>Status</th>
            <th>Embeddings</th>
            <th>Symbols</th>
            <th>Files</th>
            <th>Last Indexed</th>
            <th class="cell-actions-header">
              Actions
              <button
                class="editor-config-btn"
                :title="`Editor: ${editorCommand} (click to change)`"
                @click.stop="showEditorConfig = !showEditorConfig"
              >
                <span class="pi pi-cog"></span>
              </button>
            </th>
          </tr>
          <tr v-if="showEditorConfig" class="editor-config-row">
            <td :colspan="9">
              <div class="editor-config">
                <label>Editor command:</label>
                <input
                  v-model="editorCommand"
                  type="text"
                  class="form-input editor-input"
                  placeholder="code, code-insiders, cursor, zed..."
                  @keyup.enter="saveEditorCommand"
                  @click.stop
                />
                <button class="btn btn-primary btn-sm" @click.stop="saveEditorCommand">Save</button>
                <button class="btn btn-secondary btn-sm" @click.stop="showEditorConfig = false">Cancel</button>
              </div>
            </td>
          </tr>
        </thead>
        <tbody>
          <template v-for="p in projects" :key="p.workspace_id">
            <tr
              class="project-row"
              :class="{ 'row-expanded': expandedId === p.workspace_id }"
              @click="toggleStats(p)"
            >
              <td class="cell-chevron">
                <span
                  class="pi"
                  :class="expandedId === p.workspace_id ? 'pi-chevron-down' : 'pi-chevron-right'"
                ></span>
              </td>
              <td class="cell-name">{{ p.name }}</td>
              <td class="cell-path" :title="p.path">{{ p.path }}</td>
              <td>
                <span class="badge" :class="statusClass(p.status)">
                  {{ p.status }}
                </span>
              </td>
              <td>
                <template v-if="p.embedding_status">
                  <span
                    class="badge"
                    :class="p.embedding_status.degraded_reason ? 'badge-warning' : 'badge-ready'"
                    :title="p.embedding_status.degraded_reason ?? undefined"
                  >
                    {{ p.embedding_status.backend }}
                    <span v-if="p.embedding_status.accelerated" class="accel-icon" title="GPU accelerated">&#9889;</span>
                    <span v-if="p.embedding_status.degraded_reason" class="pi pi-exclamation-triangle degrade-icon"></span>
                  </span>
                </template>
                <span v-else class="text-muted">--</span>
              </td>
              <td class="cell-num">{{ p.symbol_count?.toLocaleString() ?? '--' }}</td>
              <td class="cell-num">{{ p.file_count?.toLocaleString() ?? '--' }}</td>
              <td class="cell-date" :title="p.last_indexed ?? undefined">{{ p.last_indexed ? timeAgo(p.last_indexed) : '--' }}</td>
              <td class="cell-actions" @click.stop>
                <button
                  class="action-btn"
                  :title="copiedPath === p.workspace_id ? 'Copied!' : 'Copy path'"
                  @click="copyPath(p, $event)"
                >
                  <span :class="copiedPath === p.workspace_id ? 'pi pi-check' : 'pi pi-copy'"></span>
                </button>
                <button
                  class="action-btn"
                  :title="`Open in ${editorCommand}`"
                  @click="openInEditor(p, $event)"
                >
                  <span class="pi pi-file-edit"></span>
                </button>
                <button
                  class="action-btn"
                  title="Open in terminal"
                  @click="openInTerminal(p, $event)"
                >
                  <span class="pi pi-desktop"></span>
                </button>
              </td>
            </tr>

            <!-- Expanded stats row -->
            <tr v-if="expandedId === p.workspace_id" class="stats-row">
              <td :colspan="9">
                <!-- Loading -->
                <div v-if="statsLoading[p.workspace_id]" class="stats-loading">
                  <span class="pi pi-spin pi-spinner"></span> Loading stats...
                </div>

                <!-- Error -->
                <div v-else-if="statsError[p.workspace_id]" class="stats-error">
                  <span class="pi pi-exclamation-triangle"></span>
                  {{ statsError[p.workspace_id] }}
                </div>

                <!-- Stats content -->
                <div v-else-if="statsCache[p.workspace_id]" class="stats-panel">
                  <!-- Language breakdown -->
                  <div class="stats-section">
                    <div class="stats-section-title">Languages</div>
                    <div class="lang-bar-container">
                      <div class="lang-bar">
                        <div
                          v-for="(lang, i) in statsCache[p.workspace_id].languages"
                          :key="lang.language"
                          class="lang-bar-segment"
                          :style="{
                            width: (lang.file_count / statsCache[p.workspace_id].total_files * 100) + '%',
                            backgroundColor: langColor(i),
                          }"
                          :title="`${lang.language}: ${pluralFiles(lang.file_count)}`"
                        ></div>
                      </div>
                      <div class="lang-legend">
                        <span
                          v-for="(lang, i) in statsCache[p.workspace_id].languages"
                          :key="lang.language"
                          class="lang-legend-item"
                        >
                          <span class="lang-dot" :style="{ backgroundColor: langColor(i) }"></span>
                          {{ lang.language }}
                          <span class="lang-count">{{ pluralFiles(lang.file_count) }}</span>
                        </span>
                      </div>
                    </div>
                  </div>

                  <!-- Symbol kinds -->
                  <div class="stats-section">
                    <div class="stats-section-title">Symbol Kinds</div>
                    <div class="kind-chips">
                      <span
                        v-for="sk in statsCache[p.workspace_id].symbol_kinds"
                        :key="sk.kind"
                        class="kind-chip"
                      >
                        {{ formatKind(sk.kind) }}
                        <span class="kind-chip-count">{{ sk.count.toLocaleString() }}</span>
                      </span>
                    </div>
                  </div>

                  <!-- Index stats -->
                  <div class="stats-section">
                    <div class="stats-section-title">Index</div>
                    <div class="index-stats">
                      <span class="index-stat">
                        <span class="pi pi-link"></span>
                        {{ statsCache[p.workspace_id].total_relationships.toLocaleString() }} relationships
                      </span>
                      <span class="index-stat">
                        <span class="pi pi-database"></span>
                        {{ statsCache[p.workspace_id].db_size_mb.toFixed(1) }} MB
                      </span>
                      <span v-if="statsCache[p.workspace_id].embedding_count > 0" class="index-stat">
                        <span class="pi pi-th-large"></span>
                        {{ statsCache[p.workspace_id].embedding_count.toLocaleString() }} embeddings
                      </span>
                    </div>
                  </div>
                </div>
              </td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 1.25rem;
}

.page-title {
  font-size: 1.5rem;
  font-weight: 600;
}

/* Buttons */
.btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 1rem;
  border: none;
  border-radius: 6px;
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-primary {
  background: var(--color-primary);
  color: white;
}

.btn-primary:hover:not(:disabled) {
  background: var(--color-primary-hover);
}

.btn-secondary {
  background: var(--hover-bg);
  color: var(--text-primary);
}

.btn-secondary:hover:not(:disabled) {
  background: var(--border-color);
}

/* Register form */
.register-form {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1rem;
  margin-bottom: 1rem;
}

.form-row {
  display: flex;
  gap: 0.5rem;
}

.form-row .form-input {
  flex: 1;
  font-family: 'SF Mono', 'Fira Code', monospace;
}

.form-error {
  margin-top: 0.5rem;
  color: var(--color-error);
  font-size: 0.8rem;
  display: flex;
  align-items: center;
  gap: 0.3rem;
}

/* Status messages */
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

/* Empty state */
.empty-state {
  text-align: center;
  padding: 3rem 1rem;
  color: var(--text-secondary);
}

.empty-icon {
  font-size: 3rem;
  margin-bottom: 1rem;
  display: block;
}

.empty-hint {
  font-size: 0.85rem;
  margin-top: 0.5rem;
}

/* Table */
.table-wrapper {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  overflow-x: auto;
}

.projects-table {
  width: 100%;
  min-width: 700px;
  border-collapse: collapse;
  font-size: 0.875rem;
}

.projects-table th {
  text-align: left;
  padding: 0.75rem 1rem;
  background: var(--hover-bg);
  border-bottom: 1px solid var(--border-color);
  font-weight: 600;
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--text-secondary);
}

.projects-table th:first-child {
  width: 2rem;
  padding-right: 0;
}

.projects-table td {
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--border-color);
}

.projects-table tr:last-child td {
  border-bottom: none;
}

.project-row {
  cursor: pointer;
  transition: background 0.1s;
}

.project-row:hover td {
  background: var(--hover-bg);
}

.row-expanded td {
  border-bottom-color: transparent;
}

.cell-chevron {
  width: 2rem;
  padding-right: 0 !important;
  color: var(--text-muted);
  font-size: 0.7rem;
}

.cell-name {
  font-weight: 600;
}

.cell-path {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.8rem;
  color: var(--text-secondary);
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cell-num {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.cell-date {
  color: var(--text-secondary);
  font-size: 0.8rem;
}

/* Actions column */
.cell-actions-header {
  text-align: center;
  white-space: nowrap;
}

.editor-config-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  padding: 0 0.25rem;
  font-size: 0.65rem;
  vertical-align: middle;
  opacity: 0.6;
  transition: opacity 0.15s;
}

.editor-config-btn:hover {
  opacity: 1;
  color: var(--text-primary);
}

.editor-config-row td {
  padding: 0.5rem 1rem !important;
  background: var(--hover-bg);
  border-bottom: 1px solid var(--border-color);
}

.editor-config {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.85rem;
  color: var(--text-secondary);
}

.editor-input {
  width: 200px;
  padding: 0.3rem 0.5rem !important;
  font-size: 0.8rem !important;
}

.btn-sm {
  padding: 0.25rem 0.6rem !important;
  font-size: 0.75rem !important;
}

.cell-actions {
  text-align: center;
  white-space: nowrap;
  padding: 0.5rem !important;
}

.action-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: all 0.15s;
  font-size: 0.8rem;
}

.action-btn:hover {
  background: var(--color-primary-bg);
  color: var(--color-primary);
}

.action-btn .pi-check {
  color: var(--color-success);
}

/* Status badges */
.badge {
  display: inline-block;
  padding: 0.15rem 0.5rem;
  border-radius: 9999px;
  font-size: 0.75rem;
  font-weight: 600;
  text-transform: capitalize;
}

.badge-ready {
  background: rgba(22, 163, 74, 0.1);
  color: var(--color-success);
}

.badge-indexing {
  background: var(--color-warning-bg);
  color: var(--color-warning);
}

.badge-registered {
  background: rgba(99, 102, 241, 0.1);
  color: var(--color-primary-hover);
}

.badge-error {
  background: var(--color-error-bg);
  color: var(--color-error);
}

.badge-default {
  background: var(--hover-bg);
  color: var(--text-secondary);
}

.badge-warning {
  background: var(--color-warning-bg);
  color: var(--color-warning);
}

.text-muted {
  color: var(--text-muted);
  font-size: 0.85rem;
}

.accel-icon {
  font-size: 0.7rem;
  margin-left: 0.15rem;
}

.degrade-icon {
  font-size: 0.6rem;
  margin-left: 0.2rem;
}

/* Stats expanded row */
.stats-row td {
  padding: 0 1rem 1rem 1rem;
  background: var(--card-bg);
}

.stats-loading,
.stats-error {
  padding: 1rem;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.85rem;
}

.stats-error {
  color: var(--color-error);
}

.stats-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 0.75rem 1rem;
  background: var(--hover-bg);
  border-radius: 8px;
}

.stats-section-title {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
  margin-bottom: 0.4rem;
}

/* Language bar */
.lang-bar {
  display: flex;
  height: 8px;
  border-radius: 4px;
  overflow: hidden;
  background: var(--border-color);
}

.lang-bar-segment {
  min-width: 2px;
  transition: width 0.3s;
}

.lang-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem 1rem;
  margin-top: 0.4rem;
}

.lang-legend-item {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.lang-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.lang-count {
  color: var(--text-muted);
  font-size: 0.7rem;
}

/* Symbol kind chips */
.kind-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
}

.kind-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  padding: 0.2rem 0.6rem;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 0.75rem;
  color: var(--text-secondary);
  text-transform: capitalize;
}

.kind-chip-count {
  font-weight: 600;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
}

/* Index stats */
.index-stats {
  display: flex;
  flex-wrap: wrap;
  gap: 1.25rem;
}

.index-stat {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.8rem;
  color: var(--text-secondary);
}

.index-stat .pi {
  font-size: 0.75rem;
  color: var(--text-muted);
}

/* Responsive */
@media (max-width: 600px) {
  .page-header {
    flex-direction: column;
    align-items: stretch;
    gap: 0.75rem;
  }

  .form-row {
    flex-direction: column;
  }

  .form-row .form-input {
    width: 100%;
  }
}
</style>
