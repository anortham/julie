<script setup lang="ts">
/**
 * SearchFilters — extracted filter controls for the Search Playground.
 *
 * Includes:
 * - Search target (definitions / content) radio group
 * - Content type selector (Code / Memories / All)
 * - Language dropdown
 * - File pattern input
 * - Limit input
 * - Hybrid (semantic search) toggle
 * - Debug mode toggle
 */

interface Project {
  workspace_id: string
  name: string
}

const searchTarget = defineModel<'definitions' | 'content'>('searchTarget', { required: true })
const language = defineModel<string>('language', { required: true })
const filePattern = defineModel<string>('filePattern', { required: true })
const limit = defineModel<number>('limit', { required: true })
const debugMode = defineModel<boolean>('debugMode', { required: true })
const contentType = defineModel<'code' | 'memory' | 'all'>('contentType', { required: true })
const hybrid = defineModel<boolean>('hybrid', { required: true })
const project = defineModel<string>('project', { default: '' })

defineProps<{
  languages: string[]
  projects: Project[]
}>()

const contentTypeOptions = [
  { label: 'Code', value: 'code' },
  { label: 'Memories', value: 'memory' },
  { label: 'All', value: 'all' },
]
</script>

<template>
  <div class="filters-row">
    <div class="filter-group">
      <label class="filter-label">Target</label>
      <div class="radio-group">
        <label class="radio-item" :class="{ active: searchTarget === 'definitions' }">
          <input
            v-model="searchTarget"
            type="radio"
            value="definitions"
            class="radio-input"
          />
          Definitions
        </label>
        <label class="radio-item" :class="{ active: searchTarget === 'content' }">
          <input
            v-model="searchTarget"
            type="radio"
            value="content"
            class="radio-input"
          />
          Content
        </label>
      </div>
    </div>

    <div class="filter-group">
      <label class="filter-label">Content</label>
      <div class="radio-group">
        <label
          v-for="opt in contentTypeOptions"
          :key="opt.value"
          class="radio-item"
          :class="{ active: contentType === opt.value }"
        >
          <input
            v-model="contentType"
            type="radio"
            :value="opt.value"
            class="radio-input"
          />
          {{ opt.label }}
        </label>
      </div>
    </div>

    <div v-if="projects.length > 1" class="filter-group">
      <label class="filter-label" for="project-select">Project</label>
      <select id="project-select" v-model="project" class="form-select">
        <option value="">All projects</option>
        <option v-for="p in projects" :key="p.workspace_id" :value="p.workspace_id">
          {{ p.name }}
        </option>
      </select>
    </div>

    <div class="filter-group">
      <label class="filter-label" for="lang-select">Language</label>
      <select id="lang-select" v-model="language" class="form-select">
        <option value="">All languages</option>
        <option v-for="lang in languages.filter(l => l)" :key="lang" :value="lang">
          {{ lang }}
        </option>
      </select>
    </div>

    <div class="filter-group">
      <label class="filter-label" for="pattern-input">File pattern</label>
      <input
        id="pattern-input"
        v-model="filePattern"
        type="text"
        placeholder="e.g. src/**/*.rs"
        class="form-input form-input-sm"
      />
    </div>

    <div class="filter-group">
      <label class="filter-label" for="limit-input">Limit</label>
      <input
        id="limit-input"
        v-model.number="limit"
        type="number"
        min="1"
        max="500"
        class="form-input form-input-num"
      />
    </div>

    <div class="filter-group filter-toggle">
      <label class="toggle-label">
        <input
          v-model="hybrid"
          type="checkbox"
          class="toggle-input"
        />
        <span class="toggle-track">
          <span class="toggle-thumb"></span>
        </span>
        <span class="toggle-text">Semantic</span>
      </label>
    </div>

    <div class="filter-group filter-toggle">
      <label class="toggle-label">
        <input
          v-model="debugMode"
          type="checkbox"
          class="toggle-input"
        />
        <span class="toggle-track">
          <span class="toggle-thumb"></span>
        </span>
        <span class="toggle-text">Debug</span>
      </label>
    </div>
  </div>
</template>

<style scoped>
.filters-row {
  display: flex;
  flex-wrap: wrap;
  gap: 1rem;
  align-items: flex-end;
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

.form-input-sm {
  width: 160px;
}

.form-input-num {
  width: 72px;
  font-variant-numeric: tabular-nums;
}

/* Radio group */
.radio-group {
  display: flex;
  gap: 0;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  overflow: hidden;
}

.radio-item {
  padding: 0.4rem 0.75rem;
  font-size: 0.8rem;
  cursor: pointer;
  user-select: none;
  border-right: 1px solid var(--border-color);
  transition: background 0.15s, color 0.15s;
  color: var(--text-secondary);
}

.radio-item:last-child {
  border-right: none;
}

.radio-item.active {
  background: var(--color-primary);
  color: white;
}

.radio-input {
  display: none;
}

/* Toggle switch */
.filter-toggle {
  justify-content: flex-end;
}

.toggle-label {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  cursor: pointer;
  user-select: none;
}

.toggle-input {
  display: none;
}

.toggle-track {
  position: relative;
  width: 36px;
  height: 20px;
  background: var(--border-color);
  border-radius: 10px;
  transition: background 0.2s;
}

.toggle-input:checked + .toggle-track {
  background: var(--color-primary);
}

.toggle-thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  background: var(--card-bg);
  border-radius: 50%;
  transition: transform 0.2s;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
}

.toggle-input:checked + .toggle-track .toggle-thumb {
  transform: translateX(16px);
}

.toggle-text {
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--text-secondary);
}
</style>
