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

const projects = ref<Project[]>([])
const error = ref<string | null>(null)
const loading = ref(true)

// Register form state
const showRegister = ref(false)
const registerPath = ref('')
const registerError = ref<string | null>(null)
const registering = ref(false)

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
            <th>Name</th>
            <th>Path</th>
            <th>Status</th>
            <th>Embeddings</th>
            <th>Symbols</th>
            <th>Files</th>
            <th>Last Indexed</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in projects" :key="p.workspace_id">
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
            <td class="cell-date">{{ p.last_indexed ?? '--' }}</td>
          </tr>
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
  overflow: hidden;
}

.projects-table {
  width: 100%;
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

.projects-table td {
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--border-color);
}

.projects-table tr:last-child td {
  border-bottom: none;
}

.projects-table tr:hover td {
  background: var(--hover-bg);
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
</style>
