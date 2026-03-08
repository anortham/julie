<script setup lang="ts">
import { ref, nextTick, onMounted, onUnmounted, computed } from 'vue'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface Project {
  workspace_id: string
  name: string
  path: string
  status: string
}

interface Backend {
  name: string
  available: boolean
  version?: string
}

interface DispatchSummary {
  id: string
  task: string
  project?: string
  status: string
  started_at: string
  completed_at?: string
  error?: string
}

interface DispatchDetail {
  id: string
  task: string
  project?: string
  status: string
  started_at: string
  completed_at?: string
  output?: string
  error?: string
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

// Dispatch form
const taskDescription = ref('')
const selectedProject = ref('')
const hintsText = ref('')
const dispatching = ref(false)
const dispatchError = ref<string | null>(null)

// Streaming output
const activeDispatchId = ref<string | null>(null)
const activeStatus = ref<string | null>(null)
const outputLines = ref<string[]>([])
const outputContainer = ref<HTMLElement | null>(null)
let eventSource: EventSource | null = null

// History
const history = ref<DispatchSummary[]>([])
const historyLoading = ref(false)
const historyError = ref<string | null>(null)
const selectedDispatch = ref<DispatchDetail | null>(null)
const loadingDetail = ref(false)

// Projects & backends
const projects = ref<Project[]>([])
const backends = ref<Backend[]>([])

// ---------------------------------------------------------------------------
// Computed
// ---------------------------------------------------------------------------

const availableBackends = computed(() => backends.value.filter((b) => b.available))
const unavailableBackends = computed(() => backends.value.filter((b) => !b.available))

const statusBadgeClass = computed(() => {
  return (status: string) => {
    const map: Record<string, string> = {
      running: 'badge-running',
      completed: 'badge-completed',
      failed: 'badge-failed',
      queued: 'badge-queued',
    }
    return map[status] ?? 'badge-default'
  }
})

// ---------------------------------------------------------------------------
// Fetch helpers
// ---------------------------------------------------------------------------

async function fetchProjects() {
  try {
    const res = await fetch('/api/projects')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    projects.value = await res.json()
  } catch (e) {
    console.warn('Failed to fetch projects:', e)
  }
}

async function fetchBackends() {
  try {
    const res = await fetch('/api/agents/backends')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const data = await res.json()
    backends.value = data.backends ?? []
  } catch (e) {
    console.warn('Failed to fetch backends:', e)
  }
}

async function fetchHistory() {
  historyLoading.value = true
  historyError.value = null
  try {
    const res = await fetch('/api/agents/history?limit=50')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const data = await res.json()
    history.value = data.dispatches ?? []
  } catch (e) {
    historyError.value = e instanceof Error ? e.message : 'Failed to fetch history'
  } finally {
    historyLoading.value = false
  }
}

async function fetchDispatchDetail(id: string) {
  loadingDetail.value = true
  try {
    const res = await fetch(`/api/agents/${id}`)
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    selectedDispatch.value = await res.json()
  } catch (e) {
    console.warn('Failed to fetch dispatch detail:', e)
  } finally {
    loadingDetail.value = false
  }
}

// ---------------------------------------------------------------------------
// Dispatch & Streaming
// ---------------------------------------------------------------------------

async function dispatchTask() {
  if (!taskDescription.value.trim()) return

  dispatching.value = true
  dispatchError.value = null
  outputLines.value = []
  activeDispatchId.value = null
  activeStatus.value = null
  selectedDispatch.value = null

  const body: Record<string, unknown> = {
    task: taskDescription.value.trim(),
  }
  if (selectedProject.value) {
    body.project = selectedProject.value
  }
  if (hintsText.value.trim()) {
    body.hints = { extra_context: hintsText.value.trim() }
  }

  try {
    const res = await fetch('/api/agents/dispatch', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (!res.ok) {
      const text = await res.text()
      throw new Error(text || `HTTP ${res.status}`)
    }
    const data = await res.json()
    activeDispatchId.value = data.id
    activeStatus.value = data.status ?? 'running'

    // Start streaming
    connectStream(data.id)
  } catch (e) {
    dispatchError.value = e instanceof Error ? e.message : 'Dispatch failed'
  } finally {
    dispatching.value = false
  }
}

function connectStream(id: string) {
  closeStream()

  eventSource = new EventSource(`/api/agents/${id}/stream`)

  eventSource.onmessage = async (event) => {
    outputLines.value.push(event.data)
    await nextTick()
    scrollToBottom()
  }

  eventSource.addEventListener('done', (event) => {
    activeStatus.value = (event as MessageEvent).data || 'completed'
    closeStream()
    fetchHistory()
  })

  eventSource.onerror = () => {
    // If we haven't received a done event yet, mark as error
    if (activeStatus.value === 'running') {
      activeStatus.value = 'failed'
    }
    closeStream()
    fetchHistory()
  }
}

function closeStream() {
  if (eventSource) {
    eventSource.close()
    eventSource = null
  }
}

function scrollToBottom() {
  if (outputContainer.value) {
    outputContainer.value.scrollTop = outputContainer.value.scrollHeight
  }
}

// ---------------------------------------------------------------------------
// History actions
// ---------------------------------------------------------------------------

function viewDispatch(dispatch: DispatchSummary) {
  fetchDispatchDetail(dispatch.id)
}

function closeDetail() {
  selectedDispatch.value = null
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

function relativeTime(iso: string): string {
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

function truncate(text: string, max: number): string {
  if (text.length <= max) return text
  return text.substring(0, max) + '...'
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

onMounted(() => {
  fetchProjects()
  fetchBackends()
  fetchHistory()
})

onUnmounted(() => {
  closeStream()
})
</script>

<template>
  <div class="agents-page">
    <h1 class="page-title">Agents</h1>

    <!-- Backend availability -->
    <div v-if="backends.length > 0" class="backends-bar">
      <span class="backends-label">Backends:</span>
      <span
        v-for="b in availableBackends"
        :key="b.name"
        class="backend-chip backend-available"
        :title="b.version ? `v${b.version}` : 'Available'"
      >
        <span class="pi pi-check-circle backend-icon"></span>
        {{ b.name }}
        <span v-if="b.version" class="backend-version">{{ b.version }}</span>
      </span>
      <span
        v-for="b in unavailableBackends"
        :key="b.name"
        class="backend-chip backend-unavailable"
        title="Unavailable"
      >
        <span class="pi pi-times-circle backend-icon"></span>
        {{ b.name }}
      </span>
    </div>

    <!-- Dispatch form -->
    <div class="dispatch-form">
      <div class="form-group">
        <label class="form-label" for="task-input">Task description</label>
        <textarea
          id="task-input"
          v-model="taskDescription"
          class="form-textarea"
          rows="3"
          placeholder="Describe the task for the agent..."
          :disabled="dispatching"
        ></textarea>
      </div>

      <div class="form-row">
        <div class="form-group form-group-project">
          <label class="form-label" for="project-select">Project</label>
          <select
            id="project-select"
            v-model="selectedProject"
            class="form-select"
            :disabled="dispatching"
          >
            <option value="">Default / auto-detect</option>
            <option v-for="p in projects" :key="p.workspace_id" :value="p.name">
              {{ p.name }}
            </option>
          </select>
        </div>

        <div class="form-group form-group-hints">
          <label class="form-label" for="hints-input">Hints (symbols, files, context)</label>
          <textarea
            id="hints-input"
            v-model="hintsText"
            class="form-textarea form-textarea-sm"
            rows="2"
            placeholder="Optional: relevant symbols, file paths, or extra context..."
            :disabled="dispatching"
          ></textarea>
        </div>
      </div>

      <div class="form-actions">
        <button
          class="btn btn-primary"
          :disabled="dispatching || !taskDescription.trim()"
          @click="dispatchTask"
        >
          <span v-if="dispatching" class="pi pi-spin pi-spinner"></span>
          <span v-else class="pi pi-play"></span>
          {{ dispatching ? 'Dispatching...' : 'Dispatch' }}
        </button>
      </div>
    </div>

    <!-- Dispatch error -->
    <div v-if="dispatchError" class="status-message status-error">
      <span class="pi pi-exclamation-triangle"></span>
      {{ dispatchError }}
    </div>

    <!-- Active dispatch output -->
    <div v-if="activeDispatchId" class="output-section">
      <div class="output-header">
        <span class="output-title">
          <span class="pi pi-desktop"></span>
          Dispatch {{ activeDispatchId }}
        </span>
        <span
          v-if="activeStatus"
          class="badge"
          :class="statusBadgeClass(activeStatus)"
        >
          <span v-if="activeStatus === 'running'" class="pi pi-spin pi-spinner badge-spinner"></span>
          {{ activeStatus }}
        </span>
      </div>
      <div ref="outputContainer" class="output-terminal">
        <div v-if="outputLines.length === 0 && activeStatus === 'running'" class="output-waiting">
          Waiting for output...
        </div>
        <div v-for="(line, i) in outputLines" :key="i" class="output-line">{{ line }}</div>
      </div>
    </div>

    <!-- Dispatch detail overlay (from history click) -->
    <div v-if="selectedDispatch" class="output-section">
      <div class="output-header">
        <span class="output-title">
          <span class="pi pi-desktop"></span>
          Dispatch {{ selectedDispatch.id }}
        </span>
        <span
          class="badge"
          :class="statusBadgeClass(selectedDispatch.status)"
        >
          {{ selectedDispatch.status }}
        </span>
        <button class="btn btn-icon" @click="closeDetail">
          <span class="pi pi-times"></span>
        </button>
      </div>
      <div class="detail-meta">
        <span class="meta-item">
          <span class="pi pi-file meta-icon"></span>
          {{ truncate(selectedDispatch.task, 120) }}
        </span>
        <span v-if="selectedDispatch.project" class="meta-item">
          <span class="pi pi-folder meta-icon"></span>
          {{ selectedDispatch.project }}
        </span>
        <span class="meta-item">
          <span class="pi pi-clock meta-icon"></span>
          {{ relativeTime(selectedDispatch.started_at) }}
        </span>
      </div>
      <div v-if="loadingDetail" class="output-terminal">
        <div class="output-waiting">Loading output...</div>
      </div>
      <div v-else class="output-terminal">
        <div v-if="selectedDispatch.output" class="output-line">{{ selectedDispatch.output }}</div>
        <div v-else class="output-waiting">No output recorded.</div>
      </div>
      <div v-if="selectedDispatch.error" class="detail-error">
        <span class="pi pi-exclamation-triangle"></span>
        {{ selectedDispatch.error }}
      </div>
    </div>

    <!-- History section -->
    <div class="history-section">
      <div class="section-header">
        <h2 class="section-title">
          <span class="pi pi-history"></span>
          Dispatch History
        </h2>
        <button class="btn btn-text" :disabled="historyLoading" @click="fetchHistory">
          <span :class="historyLoading ? 'pi pi-spin pi-spinner' : 'pi pi-refresh'"></span>
          Refresh
        </button>
      </div>

      <div v-if="historyLoading && history.length === 0" class="status-message">
        <span class="pi pi-spin pi-spinner"></span> Loading history...
      </div>

      <div v-else-if="historyError" class="status-message status-error">
        <span class="pi pi-exclamation-triangle"></span>
        {{ historyError }}
      </div>

      <div v-else-if="history.length === 0" class="empty-state">
        <span class="pi pi-inbox empty-icon"></span>
        <p>No dispatches yet.</p>
        <p class="empty-hint">Use the form above to dispatch a task to an agent.</p>
      </div>

      <div v-else class="history-list">
        <div
          v-for="d in history"
          :key="d.id"
          class="history-card"
          @click="viewDispatch(d)"
        >
          <div class="history-header">
            <span
              class="badge"
              :class="statusBadgeClass(d.status)"
            >
              <span v-if="d.status === 'running'" class="pi pi-spin pi-spinner badge-spinner"></span>
              {{ d.status }}
            </span>
            <span class="history-task">{{ truncate(d.task, 80) }}</span>
            <span class="history-time" :title="d.started_at">
              {{ relativeTime(d.started_at) }}
            </span>
          </div>
          <div class="history-meta">
            <span v-if="d.project" class="meta-item">
              <span class="pi pi-folder meta-icon"></span>
              {{ d.project }}
            </span>
            <span v-if="d.error" class="meta-item meta-error">
              <span class="pi pi-exclamation-triangle meta-icon"></span>
              {{ truncate(d.error, 60) }}
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.page-title {
  font-size: 1.5rem;
  font-weight: 600;
  margin-bottom: 1.25rem;
}

/* Backend availability bar */
.backends-bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-bottom: 1rem;
  padding: 0.6rem 1rem;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  font-size: 0.85rem;
}

.backends-label {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
}

.backend-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.2rem 0.5rem;
  border-radius: 9999px;
  font-size: 0.75rem;
  font-weight: 600;
}

.backend-available {
  background: #dcfce7;
  color: #16a34a;
}

.backend-unavailable {
  background: #fee2e2;
  color: #dc2626;
}

.backend-icon {
  font-size: 0.7rem;
}

.backend-version {
  font-size: 0.65rem;
  opacity: 0.7;
  font-weight: 400;
}

/* Dispatch form */
.dispatch-form {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1rem;
  margin-bottom: 1rem;
}

.form-group {
  margin-bottom: 0.75rem;
}

.form-group:last-child {
  margin-bottom: 0;
}

.form-label {
  display: block;
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
  margin-bottom: 0.25rem;
}

.form-textarea {
  width: 100%;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 0.875rem;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  resize: vertical;
  background: white;
  line-height: 1.5;
}

.form-textarea:focus {
  outline: none;
  border-color: #6366f1;
  box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.2);
}

.form-textarea-sm {
  font-size: 0.8rem;
}

.form-row {
  display: flex;
  gap: 1rem;
  align-items: flex-start;
}

.form-group-project {
  flex: 0 0 220px;
}

.form-group-hints {
  flex: 1;
}

.form-select {
  width: 100%;
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 0.875rem;
  background: white;
  cursor: pointer;
}

.form-select:focus {
  outline: none;
  border-color: #6366f1;
  box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.2);
}

.form-actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 0.5rem;
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
  background: #6366f1;
  color: white;
}

.btn-primary:hover:not(:disabled) {
  background: #4f46e5;
}

.btn-text {
  background: none;
  color: var(--text-secondary);
  padding: 0.25rem 0.5rem;
}

.btn-text:hover:not(:disabled) {
  color: var(--text-primary);
  background: #f1f5f9;
}

.btn-icon {
  background: none;
  color: var(--text-secondary);
  padding: 0.25rem;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  margin-left: auto;
}

.btn-icon:hover {
  background: #f1f5f9;
  color: var(--text-primary);
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
  margin-bottom: 1rem;
}

.status-error {
  border-color: #fca5a5;
  color: #dc2626;
  background: #fef2f2;
}

/* Output section */
.output-section {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  overflow: hidden;
  margin-bottom: 1.25rem;
}

.output-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--border-color);
  background: #f8fafc;
}

.output-title {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-primary);
}

.output-title .pi {
  color: var(--text-secondary);
}

.output-terminal {
  background: #1e1e1e;
  color: #d4d4d4;
  font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', 'Consolas', monospace;
  font-size: 0.8rem;
  line-height: 1.6;
  padding: 0.75rem 1rem;
  max-height: 400px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-word;
}

.output-waiting {
  color: #808080;
  font-style: italic;
}

.output-line {
  min-height: 1.6em;
}

.detail-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 1rem;
  padding: 0.5rem 1rem;
  border-bottom: 1px solid var(--border-color);
  background: #fafbfc;
}

.detail-error {
  padding: 0.6rem 1rem;
  color: #dc2626;
  font-size: 0.8rem;
  display: flex;
  align-items: center;
  gap: 0.4rem;
  background: #fef2f2;
  border-top: 1px solid #fca5a5;
}

/* Badges */
.badge {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.15rem 0.5rem;
  border-radius: 9999px;
  font-size: 0.75rem;
  font-weight: 600;
  text-transform: capitalize;
  flex-shrink: 0;
}

.badge-spinner {
  font-size: 0.65rem;
}

.badge-running {
  background: #fef3c7;
  color: #d97706;
}

.badge-completed {
  background: #dcfce7;
  color: #16a34a;
}

.badge-failed {
  background: #fee2e2;
  color: #dc2626;
}

.badge-queued {
  background: #e0e7ff;
  color: #4f46e5;
}

.badge-default {
  background: #f1f5f9;
  color: var(--text-secondary);
}

/* History section */
.history-section {
  margin-top: 1.5rem;
}

.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}

.section-title {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--text-primary);
}

.section-title .pi {
  color: var(--text-secondary);
}

.history-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.history-card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 0.75rem 1rem;
  cursor: pointer;
  transition: border-color 0.15s;
}

.history-card:hover {
  border-color: #c7d2fe;
}

.history-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.history-task {
  flex: 1;
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--text-primary);
}

.history-time {
  flex-shrink: 0;
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.history-meta {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 0.35rem;
  flex-wrap: wrap;
}

.meta-item {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.meta-icon {
  font-size: 0.7rem;
}

.meta-error {
  color: #dc2626;
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

/* Responsive */
@media (max-width: 700px) {
  .form-row {
    flex-direction: column;
  }

  .form-group-project {
    flex: none;
    width: 100%;
  }
}
</style>
