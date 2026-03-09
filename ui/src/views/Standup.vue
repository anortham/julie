<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'

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

interface ProjectStandup {
  name: string
  done: string[]
  upNext: string[]
  blocked: string[]
  planSummary: string | null
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const loading = ref(false)
const error = ref<string | null>(null)
const copied = ref(false)
const projects = ref<Project[]>([])
const projectFilter = ref('')
const rangeFilter = ref('1d')
const projectStandups = ref<ProjectStandup[]>([])

// ---------------------------------------------------------------------------
// Range options
// ---------------------------------------------------------------------------

const rangeOptions = [
  { value: '1d', label: 'Yesterday' },
  { value: '3d', label: 'Last 3 days' },
  { value: '7d', label: 'Last 7 days' },
  { value: '14d', label: 'Last 14 days' },
]

// ---------------------------------------------------------------------------
// Computed
// ---------------------------------------------------------------------------

const isMultiProject = computed(() => projectStandups.value.length > 1)

const hasContent = computed(() =>
  projectStandups.value.some(p => p.done.length > 0 || p.upNext.length > 0 || p.blocked.length > 0),
)

const standupDate = computed(() => {
  const now = new Date()
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
  if (rangeFilter.value === '1d') {
    return `${months[now.getMonth()]} ${now.getDate()}, ${now.getFullYear()}`
  }
  const daysBack = parseInt(rangeFilter.value)
  const start = new Date(now.getTime() - daysBack * 86400000)
  return `${months[start.getMonth()]} ${start.getDate()}\u2013${now.getDate()}, ${now.getFullYear()}`
})

// ---------------------------------------------------------------------------
// Data fetching
// ---------------------------------------------------------------------------

async function fetchProjects() {
  try {
    const res = await fetch('/api/projects')
    if (!res.ok) return
    const data = await res.json()
    projects.value = (data.projects ?? data ?? []).filter(
      (p: Project) => p.status === 'ready' || p.status === 'Ready',
    )
  } catch {
    // Non-fatal — single-project mode still works
  }
}

async function fetchCheckpointsForProject(projectId?: string): Promise<Checkpoint[]> {
  const params = new URLSearchParams()
  params.set('since', rangeFilter.value)
  params.set('limit', '100')
  if (projectId) params.set('project', projectId)
  const res = await fetch(`/api/memories?${params}`)
  if (!res.ok) return []
  const data: RecallResult = await res.json()
  return data.checkpoints
}

async function fetchPlansForProject(projectId?: string): Promise<Plan[]> {
  const url = projectId ? `/api/plans?project=${projectId}` : '/api/plans'
  const res = await fetch(url)
  if (!res.ok) return []
  return await res.json()
}

// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

const BLOCKER_WORDS = /\b(block|stuck|wait|blocked|waiting|impediment|can'?t proceed|dependency)\b/i

function extractSummary(cp: Checkpoint): string {
  if (cp.summary) return cp.summary
  // First meaningful line from description
  const lines = cp.description.split('\n').filter(l => l.trim() && !l.startsWith('#'))
  const first = lines[0] ?? cp.description
  return first.length > 120 ? first.substring(0, 117) + '...' : first
}

function synthesizeProject(
  name: string,
  checkpoints: Checkpoint[],
  plans: Plan[],
): ProjectStandup {
  const done: string[] = []
  const upNext: string[] = []
  const blocked: string[] = []

  // Group checkpoints by rough theme (first tag or first symbol)
  const themeGroups = new Map<string, Checkpoint[]>()
  for (const cp of checkpoints) {
    const theme = cp.tags?.[0] ?? cp.symbols?.[0] ?? '_general'
    const group = themeGroups.get(theme) ?? []
    group.push(cp)
    themeGroups.set(theme, group)
  }

  // Synthesize done items from theme groups
  for (const [, group] of themeGroups) {
    if (group.length === 1) {
      done.push(extractSummary(group[0]))
    } else {
      // Multiple checkpoints in same theme — merge into one bullet
      const summaries = group.map(cp => extractSummary(cp))
      // Use the most recent checkpoint's summary as the primary
      const primary = summaries[summaries.length - 1]
      if (group.length <= 2) {
        done.push(...summaries)
      } else {
        done.push(`${primary} (+${group.length - 1} related)`)
      }
    }
  }

  // Extract "up next" from the most recent checkpoint's next field
  const sorted = [...checkpoints].sort(
    (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime(),
  )
  for (const cp of sorted) {
    if (cp.next && upNext.length < 3) {
      upNext.push(cp.next)
    }
  }

  // Extract blockers
  for (const cp of checkpoints) {
    const text = `${cp.description} ${cp.context ?? ''} ${cp.impact ?? ''}`
    if (BLOCKER_WORDS.test(text)) {
      const summary = extractSummary(cp)
      if (!blocked.includes(summary)) {
        blocked.push(summary)
      }
    }
  }

  // Plan summary
  let planSummary: string | null = null
  const activePlan = plans.find(p => p.status === 'active')
  if (activePlan) {
    const totalTasks = (activePlan.content.match(/^- \[[ x]\]/gm) ?? []).length
    const doneTasks = (activePlan.content.match(/^- \[x\]/gm) ?? []).length
    if (totalTasks > 0) {
      planSummary = `${activePlan.title} \u2014 ${doneTasks}/${totalTasks} tasks complete`
    } else {
      planSummary = activePlan.title
    }
  }

  return { name, done, upNext, blocked, planSummary }
}

async function generateStandup() {
  loading.value = true
  error.value = null
  projectStandups.value = []

  try {
    const targetProjects =
      projectFilter.value
        ? projects.value.filter(p => p.workspace_id === projectFilter.value)
        : projects.value

    if (targetProjects.length === 0) {
      // Single-project / stdio mode — no project filter
      const [checkpoints, plans] = await Promise.all([
        fetchCheckpointsForProject(),
        fetchPlansForProject(),
      ])
      if (checkpoints.length > 0) {
        projectStandups.value = [synthesizeProject('This project', checkpoints, plans)]
      }
    } else {
      const results = await Promise.all(
        targetProjects.map(async p => {
          const [checkpoints, plans] = await Promise.all([
            fetchCheckpointsForProject(p.workspace_id),
            fetchPlansForProject(p.workspace_id),
          ])
          return { project: p, checkpoints, plans }
        }),
      )

      projectStandups.value = results
        .filter(r => r.checkpoints.length > 0)
        .map(r => synthesizeProject(r.project.name, r.checkpoints, r.plans))
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to generate standup'
  } finally {
    loading.value = false
  }
}

// ---------------------------------------------------------------------------
// Markdown generation (for copy)
// ---------------------------------------------------------------------------

const standupMarkdown = computed(() => {
  if (!hasContent.value) return ''

  const lines: string[] = [`## Standup \u2014 ${standupDate.value}`, '']

  if (isMultiProject.value) {
    for (const ps of projectStandups.value) {
      if (ps.done.length === 0 && ps.upNext.length === 0 && ps.blocked.length === 0) continue
      lines.push(`### ${ps.name}`)
      for (const item of ps.done) lines.push(`- ${item}`)
      if (ps.upNext.length > 0) {
        for (const item of ps.upNext) lines.push(`> Next: ${item}`)
      }
      if (ps.planSummary) lines.push(`> Plan: ${ps.planSummary}`)
      if (ps.blocked.length > 0) {
        for (const item of ps.blocked) lines.push(`> Blocked: ${item}`)
      }
      lines.push('')
    }
  } else {
    const ps = projectStandups.value[0]
    if (ps) {
      lines.push('### Done')
      for (const item of ps.done) lines.push(`- ${item}`)
      lines.push('')
      lines.push('### Up Next')
      if (ps.upNext.length > 0) {
        for (const item of ps.upNext) lines.push(`- ${item}`)
      } else {
        lines.push('- (none identified)')
      }
      if (ps.planSummary) lines.push(`- Plan: ${ps.planSummary}`)
      lines.push('')
      lines.push('### Blocked')
      if (ps.blocked.length > 0) {
        for (const item of ps.blocked) lines.push(`- ${item}`)
      } else {
        lines.push('- Nothing currently blocked')
      }
    }
  }

  return lines.join('\n')
})

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

async function copyStandup() {
  try {
    await navigator.clipboard.writeText(standupMarkdown.value)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch {
    // Fallback for non-HTTPS
    const ta = document.createElement('textarea')
    ta.value = standupMarkdown.value
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

onMounted(async () => {
  await fetchProjects()
  await generateStandup()
})

watch([rangeFilter, projectFilter], () => {
  generateStandup()
})
</script>

<template>
  <div class="standup-page">
    <div class="standup-header">
      <h1 class="page-title">
        <span class="pi pi-megaphone"></span>
        Standup
      </h1>
      <div class="standup-controls">
        <div v-if="projects.length > 1" class="filter-group">
          <label class="filter-label" for="standup-project">Project</label>
          <select
            id="standup-project"
            v-model="projectFilter"
            class="form-select"
          >
            <option value="">All projects</option>
            <option v-for="p in projects" :key="p.workspace_id" :value="p.workspace_id">
              {{ p.name }}
            </option>
          </select>
        </div>

        <div class="filter-group">
          <label class="filter-label" for="standup-range">Time range</label>
          <select
            id="standup-range"
            v-model="rangeFilter"
            class="form-select"
          >
            <option v-for="opt in rangeOptions" :key="opt.value" :value="opt.value">
              {{ opt.label }}
            </option>
          </select>
        </div>

        <button
          v-if="hasContent"
          class="btn btn-secondary copy-btn"
          @click="copyStandup"
        >
          <span :class="copied ? 'pi pi-check' : 'pi pi-copy'"></span>
          {{ copied ? 'Copied!' : 'Copy' }}
        </button>
      </div>
    </div>

    <!-- Loading -->
    <div v-if="loading" class="status-message">
      <span class="pi pi-spin pi-spinner"></span> Generating standup...
    </div>

    <!-- Error -->
    <div v-else-if="error" class="status-message status-error">
      <span class="pi pi-exclamation-triangle"></span>
      {{ error }}
    </div>

    <!-- Empty -->
    <div v-else-if="!hasContent" class="empty-state">
      <span class="pi pi-megaphone empty-icon"></span>
      <p>No activity recorded for this period.</p>
      <p class="empty-hint">
        Checkpoints will appear here as you work. Try expanding the time range.
      </p>
    </div>

    <!-- Standup content -->
    <div v-else class="standup-card">
      <div class="standup-title-bar">
        <h2 class="standup-date">Standup &mdash; {{ standupDate }}</h2>
      </div>

      <!-- Multi-project format -->
      <template v-if="isMultiProject">
        <div
          v-for="ps in projectStandups"
          :key="ps.name"
          class="project-section"
        >
          <h3 class="project-name">
            <span class="pi pi-folder"></span>
            {{ ps.name }}
          </h3>

          <ul class="standup-list done-list">
            <li v-for="(item, i) in ps.done" :key="'d' + i">
              <span class="bullet done-bullet"></span>
              {{ item }}
            </li>
          </ul>

          <div v-if="ps.upNext.length > 0" class="standup-forward">
            <div v-for="(item, i) in ps.upNext" :key="'n' + i" class="forward-item next-item">
              <span class="forward-label">Next:</span> {{ item }}
            </div>
          </div>

          <div v-if="ps.planSummary" class="standup-forward">
            <div class="forward-item plan-item">
              <span class="forward-label">Plan:</span> {{ ps.planSummary }}
            </div>
          </div>

          <div v-if="ps.blocked.length > 0" class="standup-forward">
            <div v-for="(item, i) in ps.blocked" :key="'b' + i" class="forward-item blocked-item">
              <span class="forward-label">Blocked:</span> {{ item }}
            </div>
          </div>
        </div>
      </template>

      <!-- Single-project format -->
      <template v-else>
        <div v-if="projectStandups[0]" class="single-project">
          <div class="section">
            <h3 class="section-heading done-heading">
              <span class="pi pi-check-circle"></span> Done
            </h3>
            <ul class="standup-list done-list">
              <li v-for="(item, i) in projectStandups[0].done" :key="'d' + i">
                <span class="bullet done-bullet"></span>
                {{ item }}
              </li>
            </ul>
          </div>

          <div class="section">
            <h3 class="section-heading next-heading">
              <span class="pi pi-arrow-right"></span> Up Next
            </h3>
            <ul class="standup-list next-list">
              <li v-for="(item, i) in projectStandups[0].upNext" :key="'n' + i">
                <span class="bullet next-bullet"></span>
                {{ item }}
              </li>
              <li v-if="projectStandups[0].upNext.length === 0" class="muted-item">
                (none identified)
              </li>
            </ul>
            <div v-if="projectStandups[0].planSummary" class="plan-line">
              <span class="pi pi-map"></span>
              {{ projectStandups[0].planSummary }}
            </div>
          </div>

          <div class="section">
            <h3 class="section-heading blocked-heading">
              <span class="pi pi-ban"></span> Blocked
            </h3>
            <ul class="standup-list blocked-list">
              <li v-for="(item, i) in projectStandups[0].blocked" :key="'b' + i">
                <span class="bullet blocked-bullet"></span>
                {{ item }}
              </li>
              <li v-if="projectStandups[0].blocked.length === 0" class="muted-item">
                Nothing currently blocked
              </li>
            </ul>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.standup-page {
  max-width: 800px;
}

/* Header */
.standup-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1.25rem;
  flex-wrap: wrap;
}

.page-title {
  font-size: 1.5rem;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin: 0;
}

.standup-controls {
  display: flex;
  align-items: flex-end;
  gap: 0.75rem;
}

.filter-group {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.filter-label {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
}

.copy-btn {
  align-self: flex-end;
  white-space: nowrap;
}

/* Status / empty — reuse global patterns */
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

/* Standup card */
.standup-card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1.5rem;
  box-shadow: var(--shadow-sm);
}

.standup-title-bar {
  margin-bottom: 1.25rem;
  padding-bottom: 0.75rem;
  border-bottom: 1px solid var(--border-color);
}

.standup-date {
  font-size: 1.1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}

/* Project sections (multi-project) */
.project-section {
  margin-bottom: 1.25rem;
  padding-bottom: 1.25rem;
  border-bottom: 1px solid var(--border-color);
}

.project-section:last-child {
  margin-bottom: 0;
  padding-bottom: 0;
  border-bottom: none;
}

.project-name {
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 0.75rem 0;
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.project-name .pi {
  color: var(--color-primary);
  font-size: 0.85rem;
}

/* Single-project sections */
.single-project .section {
  margin-bottom: 1.25rem;
}

.single-project .section:last-child {
  margin-bottom: 0;
}

.section-heading {
  font-size: 0.85rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin: 0 0 0.5rem 0;
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.done-heading { color: var(--color-success); }
.next-heading { color: var(--color-primary); }
.blocked-heading { color: var(--color-error); }

/* Lists */
.standup-list {
  list-style: none;
  padding: 0;
  margin: 0;
}

.standup-list li {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  padding: 0.3rem 0;
  font-size: 0.9rem;
  line-height: 1.45;
  color: var(--text-primary);
}

.bullet {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  margin-top: 0.45em;
}

.done-bullet { background: var(--color-success); }
.next-bullet { background: var(--color-primary); }
.blocked-bullet { background: var(--color-error); }

.muted-item {
  color: var(--text-muted);
  font-style: italic;
}

/* Forward-looking items (blockquote style) */
.standup-forward {
  margin-top: 0.5rem;
  padding-left: 0.75rem;
  border-left: 3px solid var(--border-color);
}

.forward-item {
  font-size: 0.85rem;
  padding: 0.2rem 0;
  color: var(--text-secondary);
}

.forward-label {
  font-weight: 600;
}

.next-item .forward-label { color: var(--color-primary); }
.plan-item .forward-label { color: var(--color-purple); }
.blocked-item .forward-label { color: var(--color-error); }

/* Plan line (single-project) */
.plan-line {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin-top: 0.5rem;
  font-size: 0.85rem;
  color: var(--color-purple);
  padding-left: 0.75rem;
}

/* Responsive */
@media (max-width: 600px) {
  .standup-header {
    flex-direction: column;
    align-items: stretch;
  }

  .standup-controls {
    flex-wrap: wrap;
  }
}
</style>
