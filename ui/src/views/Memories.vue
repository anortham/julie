<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface GitContext {
  branch?: string
  commit?: string
  files?: string[]
}

interface Checkpoint {
  id: string
  timestamp: string
  description: string
  type?: string
  context?: string
  decision?: string
  alternatives?: string[]
  impact?: string
  evidence?: string[]
  symbols?: string[]
  next?: string
  confidence?: number
  unknowns?: string[]
  tags?: string[]
  git?: GitContext
  summary?: string
  planId?: string
}

interface Plan {
  id: string
  title: string
  content: string
  status: string
  created: string
  updated: string
  tags: string[]
}

interface RecallResult {
  checkpoints: Checkpoint[]
  activePlan?: Plan
}

interface Project {
  workspace_id: string
  name: string
  path: string
  status: string
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const loading = ref(true)
const error = ref<string | null>(null)
const checkpoints = ref<Checkpoint[]>([])
const plans = ref<Plan[]>([])
const activePlanId = ref<string | null>(null)
const projects = ref<Project[]>([])

// Filters
const searchQuery = ref('')
const sinceFilter = ref('')
const typeFilter = ref('')
const planFilter = ref('')
const tagFilter = ref<string[]>([])
const projectFilter = ref('')

// UI state
const expandedCheckpoints = ref<Set<string>>(new Set())
const selectedPlan = ref<Plan | null>(null)
const showPlanContent = ref(false)

// ---------------------------------------------------------------------------
// Computed
// ---------------------------------------------------------------------------

const checkpointTypes = ['checkpoint', 'decision', 'incident', 'learning']

const allTags = computed(() => {
  const tagSet = new Set<string>()
  for (const cp of checkpoints.value) {
    if (cp.tags) {
      for (const tag of cp.tags) {
        tagSet.add(tag)
      }
    }
  }
  return Array.from(tagSet).sort()
})

const filteredCheckpoints = computed(() => {
  let result = checkpoints.value

  // Client-side type filter (backend doesn't support it)
  if (typeFilter.value) {
    result = result.filter(
      (cp) => cp.type?.toLowerCase() === typeFilter.value.toLowerCase(),
    )
  }

  // Client-side tag filter
  if (tagFilter.value.length > 0) {
    result = result.filter((cp) =>
      tagFilter.value.every((t) => cp.tags?.includes(t)),
    )
  }

  return result
})

// ---------------------------------------------------------------------------
// Fetch helpers
// ---------------------------------------------------------------------------

async function fetchProjects() {
  try {
    const res = await fetch('/api/projects')
    if (!res.ok) return
    const all: Project[] = await res.json()
    // Show all registered projects — memories don't require a ready index
    projects.value = all
  } catch {
    // Non-critical — selector just won't appear
  }
}

function buildMemoryParams(projectId?: string): URLSearchParams {
  const params = new URLSearchParams()
  params.set('limit', '100')
  if (sinceFilter.value) params.set('since', sinceFilter.value)
  if (searchQuery.value.trim()) params.set('search', searchQuery.value.trim())
  if (planFilter.value) params.set('planId', planFilter.value)
  if (projectId) params.set('project', projectId)
  return params
}

async function fetchCheckpoints() {
  try {
    if (projectFilter.value === '' && projects.value.length > 1) {
      // "All projects" — fetch from each and merge
      const results = await Promise.all(
        projects.value.map(async (p) => {
          const params = buildMemoryParams(p.workspace_id)
          const res = await fetch(`/api/memories?${params}`)
          if (!res.ok) return { checkpoints: [] as Checkpoint[], activePlan: undefined }
          return (await res.json()) as RecallResult
        }),
      )
      const merged: Checkpoint[] = []
      for (const r of results) {
        merged.push(...r.checkpoints)
        if (r.activePlan) activePlanId.value = r.activePlan.id
      }
      merged.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
      checkpoints.value = merged.slice(0, 200)
    } else {
      // Specific project (or single-project setup)
      const params = buildMemoryParams(projectFilter.value || undefined)
      const res = await fetch(`/api/memories?${params}`)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const data: RecallResult = await res.json()
      checkpoints.value = data.checkpoints
      if (data.activePlan) activePlanId.value = data.activePlan.id
    }
    error.value = null
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch memories'
  }
}

async function fetchPlans() {
  try {
    if (projectFilter.value === '' && projects.value.length > 1) {
      const results = await Promise.all(
        projects.value.map(async (p) => {
          const res = await fetch(`/api/plans?project=${p.workspace_id}`)
          if (!res.ok) return [] as Plan[]
          return (await res.json()) as Plan[]
        }),
      )
      plans.value = results.flat().sort((a, b) =>
        new Date(b.updated).getTime() - new Date(a.updated).getTime(),
      )
    } else {
      const url = projectFilter.value
        ? `/api/plans?project=${projectFilter.value}`
        : '/api/plans'
      const res = await fetch(url)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      plans.value = await res.json()
    }
  } catch (e) {
    console.warn('Failed to fetch plans:', e)
  }
}

async function loadData() {
  loading.value = true
  error.value = null
  await fetchProjects()
  await Promise.all([fetchCheckpoints(), fetchPlans()])
  loading.value = false
}

async function applyFilters() {
  loading.value = true
  error.value = null
  activePlanId.value = null
  await Promise.all([fetchCheckpoints(), fetchPlans()])
  loading.value = false
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

function toggleCheckpoint(id: string) {
  const s = new Set(expandedCheckpoints.value)
  if (s.has(id)) {
    s.delete(id)
  } else {
    s.add(id)
  }
  expandedCheckpoints.value = s
}

function viewPlan(plan: Plan) {
  selectedPlan.value = plan
  showPlanContent.value = true
}

function closePlanContent() {
  showPlanContent.value = false
  selectedPlan.value = null
}

function filterByPlan(planId: string) {
  planFilter.value = planId
  closePlanContent()
  applyFilters()
}

function toggleTag(tag: string) {
  const idx = tagFilter.value.indexOf(tag)
  if (idx >= 0) {
    tagFilter.value = tagFilter.value.filter((t) => t !== tag)
  } else {
    tagFilter.value = [...tagFilter.value, tag]
  }
}

function clearFilters() {
  searchQuery.value = ''
  sinceFilter.value = ''
  typeFilter.value = ''
  planFilter.value = ''
  projectFilter.value = ''
  tagFilter.value = []
  applyFilters()
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

function typeColor(type?: string): string {
  const map: Record<string, string> = {
    checkpoint: '#6366f1',
    decision: '#f59e0b',
    incident: '#dc2626',
    learning: '#10b981',
  }
  return map[(type ?? 'checkpoint').toLowerCase()] ?? '#94a3b8'
}

function planStatusClass(status: string): string {
  if (status === 'active') return 'badge-active'
  if (status === 'completed') return 'badge-completed'
  if (status === 'archived') return 'badge-archived'
  return 'badge-default'
}

function confidenceLabel(n?: number): string {
  if (!n) return ''
  const labels: Record<number, string> = {
    1: 'Very Low',
    2: 'Low',
    3: 'Medium',
    4: 'High',
    5: 'Very High',
  }
  return labels[n] ?? `${n}/5`
}

const hasActiveFilters = computed(() => {
  return (
    searchQuery.value.trim() !== '' ||
    sinceFilter.value !== '' ||
    typeFilter.value !== '' ||
    planFilter.value !== '' ||
    projectFilter.value !== '' ||
    tagFilter.value.length > 0
  )
})

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

onMounted(() => {
  loadData()
})
</script>

<template>
  <div class="memories-page">
    <h1 class="page-title">Memories</h1>

    <div class="memories-layout">
      <!-- Main content: filters + timeline -->
      <div class="timeline-section">
        <!-- Filters bar -->
        <div class="filters-bar">
          <div class="filters-row">
            <div class="filter-group filter-search">
              <label class="filter-label" for="mem-search">Search</label>
              <div class="search-row">
                <input
                  id="mem-search"
                  v-model="searchQuery"
                  type="text"
                  placeholder="Search memories..."
                  class="form-input"
                  @keyup.enter="applyFilters"
                />
                <button
                  class="btn btn-primary"
                  :disabled="loading"
                  @click="applyFilters"
                >
                  <span v-if="loading" class="pi pi-spin pi-spinner"></span>
                  <span v-else class="pi pi-search"></span>
                </button>
              </div>
            </div>

            <div v-if="projects.length > 1" class="filter-group">
              <label class="filter-label" for="project-select">Project</label>
              <select
                id="project-select"
                v-model="projectFilter"
                class="form-select"
                @change="applyFilters"
              >
                <option value="">All projects</option>
                <option v-for="p in projects" :key="p.workspace_id" :value="p.workspace_id">
                  {{ p.name }}
                </option>
              </select>
            </div>

            <div class="filter-group">
              <label class="filter-label" for="since-select">Since</label>
              <select
                id="since-select"
                v-model="sinceFilter"
                class="form-select"
                @change="applyFilters"
              >
                <option value="">All time</option>
                <option value="1h">Last hour</option>
                <option value="6h">Last 6 hours</option>
                <option value="1d">Today</option>
                <option value="3d">Last 3 days</option>
                <option value="7d">Last week</option>
                <option value="30d">Last 30 days</option>
              </select>
            </div>

            <div class="filter-group">
              <label class="filter-label" for="type-select">Type</label>
              <select
                id="type-select"
                v-model="typeFilter"
                class="form-select"
              >
                <option value="">All types</option>
                <option v-for="t in checkpointTypes" :key="t" :value="t">
                  {{ t }}
                </option>
              </select>
            </div>

            <div class="filter-group">
              <label class="filter-label" for="plan-select">Plan</label>
              <select
                id="plan-select"
                v-model="planFilter"
                class="form-select"
                @change="applyFilters"
              >
                <option value="">All plans</option>
                <option v-for="p in plans" :key="p.id" :value="p.id">
                  {{ p.title }}
                </option>
              </select>
            </div>
          </div>

          <!-- Tag multi-select -->
          <div v-if="allTags.length > 0" class="tags-filter">
            <span class="filter-label">Tags</span>
            <div class="tag-chips">
              <button
                v-for="tag in allTags"
                :key="tag"
                class="tag-chip"
                :class="{ 'tag-chip-active': tagFilter.includes(tag) }"
                @click="toggleTag(tag)"
              >
                {{ tag }}
              </button>
            </div>
          </div>

          <div v-if="hasActiveFilters" class="filters-actions">
            <button class="btn btn-text" @click="clearFilters">
              <span class="pi pi-times"></span> Clear filters
            </button>
            <span class="results-count">
              {{ filteredCheckpoints.length }} checkpoint{{ filteredCheckpoints.length === 1 ? '' : 's' }}
            </span>
          </div>
        </div>

        <!-- Loading -->
        <div v-if="loading" class="status-message">
          <span class="pi pi-spin pi-spinner"></span> Loading memories...
        </div>

        <!-- Error -->
        <div v-else-if="error" class="status-message status-error">
          <span class="pi pi-exclamation-triangle"></span>
          {{ error }}
        </div>

        <!-- Empty state -->
        <div
          v-else-if="filteredCheckpoints.length === 0"
          class="empty-state"
        >
          <span class="pi pi-clock empty-icon"></span>
          <p>No checkpoints found.</p>
          <p class="empty-hint">
            {{ hasActiveFilters ? 'Try adjusting your filters.' : 'Checkpoints will appear here as you work.' }}
          </p>
        </div>

        <!-- Timeline -->
        <div v-else class="timeline">
          <div
            v-for="cp in filteredCheckpoints"
            :key="cp.id"
            class="timeline-entry"
            :class="{ expanded: expandedCheckpoints.has(cp.id) }"
          >
            <div class="timeline-dot" :style="{ background: typeColor(cp.type) }"></div>
            <div class="timeline-card" @click="toggleCheckpoint(cp.id)">
              <div class="timeline-header">
                <span
                  class="type-badge"
                  :style="{ background: typeColor(cp.type) }"
                >
                  {{ cp.type ?? 'checkpoint' }}
                </span>
                <span class="timeline-summary">
                  {{ cp.summary ?? cp.description.substring(0, 120) }}
                </span>
                <span class="timeline-time" :title="cp.timestamp">
                  {{ relativeTime(cp.timestamp) }}
                </span>
              </div>

              <div class="timeline-meta">
                <span v-if="cp.git?.branch" class="meta-item">
                  <span class="pi pi-share-alt meta-icon"></span>
                  {{ cp.git.branch }}
                </span>
                <span v-if="cp.git?.commit" class="meta-item meta-mono">
                  {{ cp.git.commit }}
                </span>
                <span v-if="cp.confidence" class="meta-item">
                  <span class="pi pi-star meta-icon"></span>
                  {{ confidenceLabel(cp.confidence) }}
                </span>
              </div>

              <!-- Tags row -->
              <div v-if="cp.tags && cp.tags.length > 0" class="timeline-tags">
                <span v-for="tag in cp.tags" :key="tag" class="tag-badge">
                  {{ tag }}
                </span>
              </div>

              <!-- Expanded content -->
              <div v-if="expandedCheckpoints.has(cp.id)" class="timeline-detail">
                <div class="detail-section">
                  <span class="detail-label">Description</span>
                  <div class="detail-text">{{ cp.description }}</div>
                </div>

                <div v-if="cp.context" class="detail-section">
                  <span class="detail-label">Context</span>
                  <div class="detail-text">{{ cp.context }}</div>
                </div>

                <div v-if="cp.decision" class="detail-section">
                  <span class="detail-label">Decision</span>
                  <div class="detail-text">{{ cp.decision }}</div>
                </div>

                <div v-if="cp.alternatives && cp.alternatives.length > 0" class="detail-section">
                  <span class="detail-label">Alternatives Considered</span>
                  <ul class="detail-list">
                    <li v-for="(alt, i) in cp.alternatives" :key="i">{{ alt }}</li>
                  </ul>
                </div>

                <div v-if="cp.impact" class="detail-section">
                  <span class="detail-label">Impact</span>
                  <div class="detail-text">{{ cp.impact }}</div>
                </div>

                <div v-if="cp.evidence && cp.evidence.length > 0" class="detail-section">
                  <span class="detail-label">Evidence</span>
                  <ul class="detail-list">
                    <li v-for="(ev, i) in cp.evidence" :key="i">{{ ev }}</li>
                  </ul>
                </div>

                <div v-if="cp.symbols && cp.symbols.length > 0" class="detail-section">
                  <span class="detail-label">Symbols</span>
                  <div class="symbol-chips">
                    <code v-for="sym in cp.symbols" :key="sym" class="symbol-chip">
                      {{ sym }}
                    </code>
                  </div>
                </div>

                <div v-if="cp.unknowns && cp.unknowns.length > 0" class="detail-section">
                  <span class="detail-label">Unknowns</span>
                  <ul class="detail-list">
                    <li v-for="(u, i) in cp.unknowns" :key="i">{{ u }}</li>
                  </ul>
                </div>

                <div v-if="cp.next" class="detail-section">
                  <span class="detail-label">Next</span>
                  <div class="detail-text">{{ cp.next }}</div>
                </div>

                <div v-if="cp.git?.files && cp.git.files.length > 0" class="detail-section">
                  <span class="detail-label">Changed Files</span>
                  <div class="file-list">
                    <code v-for="f in cp.git.files" :key="f" class="file-item">
                      {{ f }}
                    </code>
                  </div>
                </div>

                <div v-if="cp.planId" class="detail-section">
                  <span class="detail-label">Plan</span>
                  <button class="btn btn-link" @click.stop="filterByPlan(cp.planId!)">
                    {{ plans.find(p => p.id === cp.planId)?.title ?? cp.planId }}
                  </button>
                </div>

                <div class="detail-footer">
                  <span class="detail-id">{{ cp.id }}</span>
                  <span class="detail-timestamp">{{ cp.timestamp }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Plans sidebar -->
      <aside class="plans-sidebar">
        <h2 class="sidebar-title">
          <span class="pi pi-map"></span>
          Plans
        </h2>

        <div v-if="plans.length === 0" class="sidebar-empty">
          No plans yet.
        </div>

        <div v-else class="plans-list">
          <div
            v-for="plan in plans"
            :key="plan.id"
            class="plan-card"
            :class="{ 'plan-active': plan.id === activePlanId }"
            @click="viewPlan(plan)"
          >
            <div class="plan-header">
              <span class="plan-title">{{ plan.title }}</span>
              <span
                class="plan-status"
                :class="planStatusClass(plan.status)"
              >
                {{ plan.status }}
              </span>
            </div>
            <div class="plan-meta">
              <span class="plan-date">{{ relativeTime(plan.updated) }}</span>
              <span v-if="plan.tags.length > 0" class="plan-tag-count">
                {{ plan.tags.length }} tag{{ plan.tags.length === 1 ? '' : 's' }}
              </span>
            </div>
          </div>
        </div>

        <!-- Plan content overlay -->
        <div v-if="showPlanContent && selectedPlan" class="plan-overlay">
          <div class="plan-overlay-header">
            <h3 class="plan-overlay-title">{{ selectedPlan.title }}</h3>
            <button class="btn btn-icon" @click="closePlanContent">
              <span class="pi pi-times"></span>
            </button>
          </div>
          <div class="plan-overlay-meta">
            <span
              class="plan-status"
              :class="planStatusClass(selectedPlan.status)"
            >
              {{ selectedPlan.status }}
            </span>
            <span class="plan-date">Updated {{ relativeTime(selectedPlan.updated) }}</span>
          </div>
          <div v-if="selectedPlan.tags.length > 0" class="plan-overlay-tags">
            <span
              v-for="tag in selectedPlan.tags"
              :key="tag"
              class="tag-badge"
            >
              {{ tag }}
            </span>
          </div>
          <div class="plan-overlay-content">{{ selectedPlan.content }}</div>
          <div class="plan-overlay-actions">
            <button
              class="btn btn-primary btn-sm"
              @click="filterByPlan(selectedPlan.id)"
            >
              <span class="pi pi-filter"></span>
              Show linked checkpoints
            </button>
          </div>
        </div>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.page-title {
  font-size: 1.5rem;
  font-weight: 600;
  margin-bottom: 1.25rem;
}

/* Layout */
.memories-layout {
  display: grid;
  grid-template-columns: 1fr 280px;
  gap: 1.25rem;
  align-items: start;
}

@media (max-width: 900px) {
  .memories-layout {
    grid-template-columns: 1fr;
  }
}

/* Filters bar */
.filters-bar {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1rem;
  margin-bottom: 1rem;
}

.filters-row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  align-items: flex-end;
}

.filter-group {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.filter-search {
  flex: 1;
  min-width: 200px;
}

.filter-label {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
}

.search-row {
  display: flex;
  gap: 0.4rem;
}

.search-row .form-input {
  flex: 1;
}

/* Tags filter */
.tags-filter {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.75rem;
  flex-wrap: wrap;
}

.tag-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}

.tag-chip {
  padding: 0.2rem 0.5rem;
  border-radius: 9999px;
  font-size: 0.75rem;
  font-weight: 500;
  border: 1px solid var(--border-color);
  background: var(--card-bg);
  cursor: pointer;
  transition: all 0.15s;
  color: var(--text-secondary);
}

.tag-chip:hover {
  border-color: var(--brand-color);
  color: var(--color-primary);
}

.tag-chip-active {
  background: var(--color-primary);
  color: white;
  border-color: var(--color-primary);
}

.tag-chip-active:hover {
  background: var(--color-primary-hover);
  border-color: var(--color-primary-hover);
  color: white;
}

.filters-actions {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 0.75rem;
}

.results-count {
  font-size: 0.8rem;
  color: var(--text-secondary);
  font-weight: 600;
}

/* Buttons */
.btn {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 0.75rem;
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

.btn-sm {
  padding: 0.35rem 0.6rem;
  font-size: 0.8rem;
}

.btn-text {
  background: none;
  color: var(--text-secondary);
  padding: 0.25rem 0.5rem;
}

.btn-text:hover {
  color: var(--text-primary);
  background: var(--hover-bg);
}

.btn-link {
  background: none;
  color: var(--color-primary);
  padding: 0;
  font-size: 0.85rem;
  text-decoration: underline;
  cursor: pointer;
}

.btn-link:hover {
  color: var(--color-primary-hover);
}

.btn-icon {
  background: none;
  color: var(--text-secondary);
  padding: 0.25rem;
  border-radius: 4px;
}

.btn-icon:hover {
  background: var(--hover-bg);
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

/* Timeline */
.timeline {
  position: relative;
  padding-left: 1.5rem;
}

.timeline::before {
  content: '';
  position: absolute;
  left: 7px;
  top: 0;
  bottom: 0;
  width: 2px;
  background: var(--border-color);
}

.timeline-entry {
  position: relative;
  margin-bottom: 0.75rem;
}

.timeline-dot {
  position: absolute;
  left: -1.5rem;
  top: 1rem;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: 2px solid var(--card-bg);
  z-index: 1;
  box-shadow: 0 0 0 2px var(--border-color);
}

.timeline-card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 0.75rem 1rem;
  cursor: pointer;
  transition: border-color 0.15s;
}

.timeline-card:hover {
  border-color: var(--color-primary-border);
}

.timeline-entry.expanded .timeline-card {
  border-color: var(--brand-color);
}

.timeline-header {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
}

.type-badge {
  display: inline-block;
  padding: 0.1rem 0.45rem;
  border-radius: 4px;
  font-size: 0.7rem;
  font-weight: 600;
  color: white;
  text-transform: lowercase;
  flex-shrink: 0;
  margin-top: 0.1rem;
}

.timeline-summary {
  flex: 1;
  font-size: 0.9rem;
  font-weight: 500;
  line-height: 1.4;
  color: var(--text-primary);
}

.timeline-time {
  flex-shrink: 0;
  font-size: 0.75rem;
  color: var(--text-secondary);
  margin-top: 0.1rem;
}

.timeline-meta {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  margin-top: 0.4rem;
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

.meta-mono {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.7rem;
}

/* Tags */
.timeline-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
  margin-top: 0.4rem;
}

.tag-badge {
  display: inline-block;
  padding: 0.1rem 0.4rem;
  border-radius: 9999px;
  font-size: 0.65rem;
  font-weight: 600;
  background: var(--color-primary-bg);
  color: var(--color-primary);
}

/* Expanded detail */
.timeline-detail {
  margin-top: 0.75rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--border-color);
}

.detail-section {
  margin-bottom: 0.75rem;
}

.detail-section:last-child {
  margin-bottom: 0;
}

.detail-label {
  display: block;
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
  margin-bottom: 0.25rem;
}

.detail-text {
  font-size: 0.85rem;
  line-height: 1.5;
  color: var(--text-primary);
  white-space: pre-wrap;
}

.detail-list {
  margin: 0;
  padding-left: 1.25rem;
  font-size: 0.85rem;
  line-height: 1.5;
  color: var(--text-primary);
}

.detail-list li {
  margin-bottom: 0.2rem;
}

.symbol-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}

.symbol-chip {
  padding: 0.15rem 0.45rem;
  border-radius: 4px;
  font-size: 0.75rem;
  background: var(--code-bg);
  color: var(--color-primary);
  border: 1px solid var(--border-color);
}

.file-list {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
}

.file-item {
  font-size: 0.75rem;
  font-family: 'SF Mono', 'Fira Code', monospace;
  color: var(--text-secondary);
  padding: 0.15rem 0.4rem;
  background: var(--code-bg);
  border-radius: 4px;
  display: inline-block;
}

.detail-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 0.75rem;
  padding-top: 0.5rem;
  border-top: 1px dashed var(--border-color);
}

.detail-id,
.detail-timestamp {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.detail-id {
  font-family: 'SF Mono', 'Fira Code', monospace;
}

/* Plans sidebar */
.plans-sidebar {
  position: sticky;
  top: 1.5rem;
}

.sidebar-title {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 1rem;
  font-weight: 600;
  margin-bottom: 0.75rem;
  color: var(--text-primary);
}

.sidebar-title .pi {
  color: var(--text-secondary);
}

.sidebar-empty {
  color: var(--text-secondary);
  font-size: 0.85rem;
  padding: 1rem;
  text-align: center;
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
}

.plans-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.plan-card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 0.75rem;
  cursor: pointer;
  transition: border-color 0.15s;
}

.plan-card:hover {
  border-color: var(--color-primary-border);
}

.plan-active {
  border-left: 3px solid var(--color-primary);
}

.plan-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.5rem;
}

.plan-title {
  font-size: 0.85rem;
  font-weight: 600;
  line-height: 1.3;
}

.plan-status {
  display: inline-block;
  padding: 0.1rem 0.4rem;
  border-radius: 9999px;
  font-size: 0.65rem;
  font-weight: 600;
  text-transform: capitalize;
  flex-shrink: 0;
}

.badge-active {
  background: var(--color-primary-bg);
  color: var(--color-success);
}

.badge-completed {
  background: var(--color-primary-bg);
  color: var(--color-primary-hover);
}

.badge-archived,
.badge-default {
  background: var(--code-bg);
  color: var(--text-secondary);
}

.plan-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.35rem;
}

.plan-date {
  font-size: 0.7rem;
  color: var(--text-secondary);
}

.plan-tag-count {
  font-size: 0.65rem;
  color: var(--text-muted);
}

/* Plan content overlay */
.plan-overlay {
  background: var(--card-bg);
  border: 1px solid var(--brand-color);
  border-radius: 8px;
  padding: 1rem;
  margin-top: 0.75rem;
}

.plan-overlay-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.plan-overlay-title {
  font-size: 1rem;
  font-weight: 600;
}

.plan-overlay-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.5rem;
}

.plan-overlay-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
  margin-bottom: 0.75rem;
}

.plan-overlay-content {
  font-size: 0.8rem;
  line-height: 1.6;
  color: var(--text-primary);
  white-space: pre-wrap;
  max-height: 300px;
  overflow-y: auto;
  padding: 0.75rem;
  background: var(--code-bg);
  border-radius: 6px;
  border: 1px solid var(--border-color);
}

.plan-overlay-actions {
  margin-top: 0.75rem;
}
</style>
