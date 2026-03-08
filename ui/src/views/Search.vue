<script setup lang="ts">
import { ref, computed } from 'vue'
import SearchFilters from '../components/SearchFilters.vue'
import MemoryResults from '../components/MemoryResults.vue'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface SymbolResult {
  content_type?: string
  id: string
  name: string
  signature: string
  doc_comment?: string
  file_path: string
  kind: string
  language: string
  start_line: number
  score: number
}

interface ContentResult {
  file_path: string
  language: string
  score: number
}

interface MemoryResult {
  content_type: string
  id: string
  body: string
  tags?: string
  symbols?: string
  decision?: string
  impact?: string
  branch?: string
  timestamp?: string
  file_path?: string
  score: number
}

interface SearchResponse {
  search_target: string
  relaxed: boolean
  count: number
  symbols?: SymbolResult[]
  content?: ContentResult[]
  memories?: MemoryResult[]
}

interface SymbolDebugResult {
  content_type?: string
  id: string
  name: string
  signature: string
  doc_comment: string
  file_path: string
  kind: string
  language: string
  start_line: number
  final_score: number
  bm25_score: number
  centrality_score: number
  centrality_boost: number
  pattern_boost: number
  nl_path_boost: number
  field_matches: string[]
  query_tokens: string[]
  relaxed: boolean
  boost_explanation: string
}

interface ContentDebugResult {
  file_path: string
  language: string
  final_score: number
  bm25_score: number
  query_tokens: string[]
  relaxed: boolean
}

interface DebugSearchResponse {
  search_target: string
  relaxed: boolean
  count: number
  query_tokens: string[]
  hybrid_mode: boolean
  symbols?: {
    results: SymbolDebugResult[]
    relaxed: boolean
    query_tokens: string[]
    total_candidates: number
  }
  content?: {
    results: ContentDebugResult[]
    relaxed: boolean
    query_tokens: string[]
  }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

const query = ref('')
const language = ref('')
const filePattern = ref('')
const searchTarget = ref<'definitions' | 'content'>('definitions')
const debugMode = ref(false)
const limit = ref(20)
const contentType = ref<'code' | 'memory' | 'all'>('code')
const hybrid = ref(false)

const loading = ref(false)
const error = ref<string | null>(null)

// Standard results
const standardResponse = ref<SearchResponse | null>(null)

// Debug results
const debugResponse = ref<DebugSearchResponse | null>(null)

// Track which debug rows are expanded
const expandedRows = ref<Set<string>>(new Set())

const languages = [
  '', 'bash', 'c', 'cpp', 'csharp', 'css', 'dart', 'gdscript', 'go',
  'html', 'java', 'javascript', 'json', 'jsonl', 'kotlin', 'lua',
  'markdown', 'php', 'powershell', 'python', 'qml', 'r', 'razor',
  'regex', 'ruby', 'rust', 'sql', 'swift', 'toml', 'typescript',
  'vue', 'yaml', 'zig',
]

const resultCount = computed(() => {
  if (debugMode.value && debugResponse.value) return debugResponse.value.count
  if (!debugMode.value && standardResponse.value) return standardResponse.value.count
  return 0
})

const wasRelaxed = computed(() => {
  if (debugMode.value && debugResponse.value) return debugResponse.value.relaxed
  if (!debugMode.value && standardResponse.value) return standardResponse.value.relaxed
  return false
})

const queryTokens = computed(() => {
  if (debugMode.value && debugResponse.value) return debugResponse.value.query_tokens
  return []
})

const isHybridMode = computed(() => {
  if (debugMode.value && debugResponse.value) return debugResponse.value.hybrid_mode
  return false
})

const searched = ref(false)

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

async function doSearch() {
  if (!query.value.trim()) return

  loading.value = true
  error.value = null
  standardResponse.value = null
  debugResponse.value = null
  expandedRows.value = new Set()
  searched.value = true

  const endpoint = debugMode.value ? '/api/search/debug' : '/api/search'
  const body: Record<string, unknown> = {
    query: query.value.trim(),
    search_target: searchTarget.value,
    limit: limit.value,
  }
  if (language.value) body.language = language.value
  if (filePattern.value.trim()) body.file_pattern = filePattern.value.trim()
  if (contentType.value !== 'code') body.content_type = contentType.value
  if (hybrid.value) body.hybrid = true

  try {
    const res = await fetch(endpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (!res.ok) {
      const text = await res.text()
      throw new Error(text || `HTTP ${res.status}`)
    }
    if (debugMode.value) {
      debugResponse.value = await res.json()
    } else {
      standardResponse.value = await res.json()
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Search failed'
  } finally {
    loading.value = false
  }
}

function toggleRow(id: string) {
  const s = new Set(expandedRows.value)
  if (s.has(id)) {
    s.delete(id)
  } else {
    s.add(id)
  }
  expandedRows.value = s
}

function kindColor(kind: string): string {
  const map: Record<string, string> = {
    function: '#6366f1',
    method: '#818cf8',
    struct: '#f59e0b',
    class: '#f59e0b',
    enum: '#10b981',
    trait: '#ec4899',
    interface: '#ec4899',
    type: '#8b5cf6',
    constant: '#64748b',
    variable: '#64748b',
    module: '#06b6d4',
    field: '#94a3b8',
    property: '#94a3b8',
    import: '#a3a3a3',
  }
  return map[kind.toLowerCase()] ?? '#94a3b8'
}

function formatScore(n: number): string {
  return n.toFixed(4)
}

function contentTypeBadgeClass(ct?: string): string {
  if (ct === 'memory') return 'ct-badge ct-badge-memory'
  return 'ct-badge ct-badge-code'
}
</script>

<template>
  <div class="search-page">
    <h1 class="page-title">Search Playground</h1>

    <!-- Search Form -->
    <div class="search-form">
      <div class="search-row">
        <input
          v-model="query"
          type="text"
          placeholder="Search query (e.g. getUserData, search_symbols, &quot;workspace routing&quot;)"
          class="form-input search-input"
          @keyup.enter="doSearch"
        />
        <button
          class="btn btn-primary"
          :disabled="loading || !query.trim()"
          @click="doSearch"
        >
          <span v-if="loading" class="pi pi-spin pi-spinner"></span>
          <span v-else class="pi pi-search"></span>
          Search
        </button>
      </div>

      <!-- Filters (extracted component) -->
      <SearchFilters
        v-model:search-target="searchTarget"
        v-model:language="language"
        v-model:file-pattern="filePattern"
        v-model:limit="limit"
        v-model:debug-mode="debugMode"
        v-model:content-type="contentType"
        v-model:hybrid="hybrid"
        :languages="languages"
      />
    </div>

    <!-- Error -->
    <div v-if="error" class="status-message status-error">
      <span class="pi pi-exclamation-triangle"></span>
      {{ error }}
    </div>

    <!-- Loading -->
    <div v-if="loading" class="status-message">
      <span class="pi pi-spin pi-spinner"></span> Searching...
    </div>

    <!-- Token breakdown + debug badges -->
    <div v-if="debugMode && searched && !loading && !error" class="debug-header">
      <div v-if="queryTokens.length > 0" class="token-breakdown">
        <span class="token-label">Query tokens:</span>
        <code class="token-raw">{{ query }}</code>
        <span class="pi pi-arrow-right token-arrow"></span>
        <span v-for="(token, i) in queryTokens" :key="i" class="token-chip">{{ token }}</span>
      </div>
      <div class="debug-badges">
        <span
          class="mode-badge"
          :class="isHybridMode ? 'mode-badge-hybrid' : 'mode-badge-keyword'"
        >
          {{ isHybridMode ? 'Hybrid' : 'Keyword Only' }}
        </span>
        <span v-if="contentType !== 'code'" class="mode-badge mode-badge-content">
          {{ contentType === 'all' ? 'Code + Memories' : 'Memories Only' }}
        </span>
      </div>
    </div>

    <!-- Results meta -->
    <div v-if="!loading && searched && !error" class="results-meta">
      <span class="results-count">{{ resultCount }} result{{ resultCount === 1 ? '' : 's' }}</span>
      <span v-if="wasRelaxed" class="relaxed-badge">OR fallback</span>
    </div>

    <!-- Empty state -->
    <div v-if="!loading && searched && !error && resultCount === 0" class="empty-state">
      <span class="pi pi-search empty-icon"></span>
      <p>No results found.</p>
      <p class="empty-hint">Try a different query or broaden your filters.</p>
    </div>

    <!-- ================================================================= -->
    <!-- STANDARD MODE: Definition results                                   -->
    <!-- ================================================================= -->
    <div
      v-if="!debugMode && standardResponse?.symbols"
      class="results-list"
    >
      <div v-for="sym in standardResponse.symbols" :key="sym.id" class="result-card">
        <div class="result-header">
          <span
            v-if="contentType !== 'code'"
            :class="contentTypeBadgeClass(sym.content_type)"
          >
            {{ sym.content_type ?? 'code' }}
          </span>
          <span class="kind-badge" :style="{ background: kindColor(sym.kind) }">
            {{ sym.kind }}
          </span>
          <span class="result-name">{{ sym.name }}</span>
          <span class="result-score">{{ formatScore(sym.score) }}</span>
        </div>
        <div v-if="sym.signature" class="result-signature">{{ sym.signature }}</div>
        <div class="result-file">
          <span class="pi pi-file result-file-icon"></span>
          {{ sym.file_path }}<span class="result-line">:{{ sym.start_line }}</span>
          <span class="result-lang">{{ sym.language }}</span>
        </div>
      </div>
    </div>

    <!-- STANDARD MODE: Content results -->
    <div
      v-if="!debugMode && standardResponse?.content"
      class="results-list"
    >
      <div v-for="(cr, idx) in standardResponse.content" :key="idx" class="result-card">
        <div class="result-header">
          <span class="kind-badge" style="background: #64748b">file</span>
          <span class="result-name">{{ cr.file_path }}</span>
          <span class="result-score">{{ formatScore(cr.score) }}</span>
        </div>
        <div class="result-file">
          <span class="result-lang">{{ cr.language }}</span>
        </div>
      </div>
    </div>

    <!-- STANDARD MODE: Memory results -->
    <MemoryResults
      v-if="!debugMode && standardResponse?.memories && standardResponse.memories.length > 0"
      :memories="standardResponse.memories"
    />

    <!-- ================================================================= -->
    <!-- DEBUG MODE: Definition results with expandable scoring              -->
    <!-- ================================================================= -->
    <div
      v-if="debugMode && debugResponse?.symbols"
      class="results-list"
    >
      <div
        v-for="sym in debugResponse.symbols.results"
        :key="sym.id"
        class="result-card result-card-debug"
        :class="{ expanded: expandedRows.has(sym.id) }"
      >
        <div class="result-header result-header-clickable" @click="toggleRow(sym.id)">
          <span
            class="pi expand-icon"
            :class="expandedRows.has(sym.id) ? 'pi-chevron-down' : 'pi-chevron-right'"
          ></span>
          <span
            v-if="contentType !== 'code'"
            :class="contentTypeBadgeClass(sym.content_type)"
          >
            {{ sym.content_type ?? 'code' }}
          </span>
          <span class="kind-badge" :style="{ background: kindColor(sym.kind) }">
            {{ sym.kind }}
          </span>
          <span class="result-name">{{ sym.name }}</span>
          <span class="result-score">{{ formatScore(sym.final_score) }}</span>
        </div>
        <div v-if="sym.signature" class="result-signature">{{ sym.signature }}</div>
        <div class="result-file">
          <span class="pi pi-file result-file-icon"></span>
          {{ sym.file_path }}<span class="result-line">:{{ sym.start_line }}</span>
          <span class="result-lang">{{ sym.language }}</span>
        </div>

        <!-- Expanded debug panel -->
        <div v-if="expandedRows.has(sym.id)" class="debug-panel">
          <div class="debug-grid">
            <div class="debug-cell">
              <span class="debug-label">BM25</span>
              <span class="debug-value">{{ formatScore(sym.bm25_score) }}</span>
            </div>
            <div class="debug-cell">
              <span class="debug-label">Centrality</span>
              <span class="debug-value">{{ sym.centrality_score.toFixed(4) }}</span>
            </div>
            <div class="debug-cell">
              <span class="debug-label">Centrality Boost</span>
              <span class="debug-value">x{{ sym.centrality_boost.toFixed(4) }}</span>
            </div>
            <div class="debug-cell">
              <span class="debug-label">Pattern Boost</span>
              <span class="debug-value">x{{ sym.pattern_boost.toFixed(2) }}</span>
            </div>
            <div class="debug-cell">
              <span class="debug-label">NL Path Boost</span>
              <span class="debug-value">x{{ sym.nl_path_boost.toFixed(2) }}</span>
            </div>
            <div class="debug-cell">
              <span class="debug-label">Final Score</span>
              <span class="debug-value debug-value-final">{{ formatScore(sym.final_score) }}</span>
            </div>
          </div>

          <div v-if="sym.field_matches.length > 0" class="debug-section">
            <span class="debug-label">Field matches:</span>
            <span v-for="(fm, i) in sym.field_matches" :key="i" class="field-match-chip">
              {{ fm }}
            </span>
          </div>

          <div class="debug-section">
            <span class="debug-label">Boost explanation:</span>
            <code class="debug-explanation">{{ sym.boost_explanation }}</code>
          </div>
        </div>
      </div>
    </div>

    <!-- DEBUG MODE: Content results -->
    <div
      v-if="debugMode && debugResponse?.content"
      class="results-list"
    >
      <div
        v-for="(cr, idx) in debugResponse.content.results"
        :key="idx"
        class="result-card result-card-debug"
        :class="{ expanded: expandedRows.has(`content-${idx}`) }"
      >
        <div class="result-header result-header-clickable" @click="toggleRow(`content-${idx}`)">
          <span
            class="pi expand-icon"
            :class="expandedRows.has(`content-${idx}`) ? 'pi-chevron-down' : 'pi-chevron-right'"
          ></span>
          <span class="kind-badge" style="background: #64748b">file</span>
          <span class="result-name">{{ cr.file_path }}</span>
          <span class="result-score">{{ formatScore(cr.final_score) }}</span>
        </div>
        <div class="result-file">
          <span class="result-lang">{{ cr.language }}</span>
        </div>

        <div v-if="expandedRows.has(`content-${idx}`)" class="debug-panel">
          <div class="debug-grid">
            <div class="debug-cell">
              <span class="debug-label">BM25</span>
              <span class="debug-value">{{ formatScore(cr.bm25_score) }}</span>
            </div>
            <div class="debug-cell">
              <span class="debug-label">Final Score</span>
              <span class="debug-value debug-value-final">{{ formatScore(cr.final_score) }}</span>
            </div>
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

/* Search form */
.search-form {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1rem;
  margin-bottom: 1rem;
}

.search-row {
  display: flex;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
}

.search-input {
  flex: 1;
}

/* Form inputs */
.form-input {
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 0.875rem;
  font-family: 'SF Mono', 'Fira Code', monospace;
  background: white;
}

.form-input:focus {
  outline: none;
  border-color: #6366f1;
  box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.2);
}

/* Button */
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

/* Debug header — tokens + mode badges */
.debug-header {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-bottom: 1rem;
}

/* Token breakdown */
.token-breakdown {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.4rem;
  padding: 0.75rem 1rem;
  background: #f0f0ff;
  border: 1px solid #c7d2fe;
  border-radius: 8px;
  font-size: 0.85rem;
}

.token-label {
  font-weight: 600;
  color: var(--text-secondary);
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.token-raw {
  background: white;
  padding: 0.15rem 0.5rem;
  border-radius: 4px;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.8rem;
  border: 1px solid var(--border-color);
}

.token-arrow {
  color: var(--text-secondary);
  font-size: 0.7rem;
}

.token-chip {
  background: #6366f1;
  color: white;
  padding: 0.15rem 0.5rem;
  border-radius: 4px;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.8rem;
  font-weight: 500;
}

/* Debug mode badges */
.debug-badges {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.mode-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.25rem 0.6rem;
  border-radius: 9999px;
  font-size: 0.75rem;
  font-weight: 600;
}

.mode-badge-hybrid {
  background: #dcfce7;
  color: #16a34a;
}

.mode-badge-keyword {
  background: #f1f5f9;
  color: #64748b;
}

.mode-badge-content {
  background: #f3e8ff;
  color: #7c3aed;
}

/* Results meta */
.results-meta {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
  font-size: 0.85rem;
}

.results-count {
  color: var(--text-secondary);
  font-weight: 600;
}

.relaxed-badge {
  display: inline-block;
  padding: 0.1rem 0.4rem;
  border-radius: 9999px;
  font-size: 0.7rem;
  font-weight: 600;
  background: #fef3c7;
  color: #d97706;
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

/* Results list */
.results-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.result-card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 0.75rem 1rem;
  transition: border-color 0.15s;
}

.result-card:hover {
  border-color: #c7d2fe;
}

.result-card-debug.expanded {
  border-color: #818cf8;
}

.result-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.result-header-clickable {
  cursor: pointer;
}

.expand-icon {
  font-size: 0.7rem;
  color: var(--text-secondary);
  width: 14px;
  flex-shrink: 0;
}

.kind-badge {
  display: inline-block;
  padding: 0.1rem 0.45rem;
  border-radius: 4px;
  font-size: 0.7rem;
  font-weight: 600;
  color: white;
  text-transform: lowercase;
  flex-shrink: 0;
}

/* Content-type badges for mixed results */
.ct-badge {
  display: inline-block;
  padding: 0.1rem 0.4rem;
  border-radius: 9999px;
  font-size: 0.6rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  flex-shrink: 0;
}

.ct-badge-code {
  background: #dbeafe;
  color: #2563eb;
}

.ct-badge-memory {
  background: #f3e8ff;
  color: #7c3aed;
}

.result-name {
  font-weight: 600;
  font-size: 0.95rem;
  word-break: break-all;
}

.result-score {
  margin-left: auto;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.75rem;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.result-signature {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.8rem;
  color: var(--text-secondary);
  margin-top: 0.35rem;
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.4;
  max-height: 3.6em;
  overflow: hidden;
}

.result-file {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  margin-top: 0.35rem;
  font-size: 0.8rem;
  color: var(--text-secondary);
}

.result-file-icon {
  font-size: 0.75rem;
}

.result-line {
  color: #94a3b8;
}

.result-lang {
  margin-left: auto;
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #94a3b8;
  font-weight: 600;
}

/* Debug panel */
.debug-panel {
  margin-top: 0.75rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--border-color);
}

.debug-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  gap: 0.5rem;
}

.debug-cell {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  padding: 0.4rem 0.6rem;
  background: #f8fafc;
  border-radius: 6px;
}

.debug-label {
  font-size: 0.65rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text-secondary);
  font-weight: 600;
}

.debug-value {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.85rem;
  font-weight: 600;
}

.debug-value-final {
  color: #6366f1;
}

.debug-section {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.3rem;
  margin-top: 0.5rem;
}

.field-match-chip {
  display: inline-block;
  padding: 0.1rem 0.4rem;
  border-radius: 4px;
  font-size: 0.75rem;
  font-weight: 500;
  background: #dcfce7;
  color: #16a34a;
}

.debug-explanation {
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.75rem;
  background: #f8fafc;
  padding: 0.3rem 0.5rem;
  border-radius: 4px;
  color: var(--text-primary);
  word-break: break-all;
}
</style>
