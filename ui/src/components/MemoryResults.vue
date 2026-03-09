<script setup lang="ts">
/**
 * MemoryResults — renders memory search result cards.
 *
 * Each card shows: body snippet, tags, symbols, timestamp, score,
 * and optional decision/impact fields when expanded.
 */

import { ref } from 'vue'

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

defineProps<{
  memories: MemoryResult[]
  debugMode?: boolean
}>()

const expandedIds = ref<Set<string>>(new Set())

function toggleExpand(id: string) {
  const s = new Set(expandedIds.value)
  if (s.has(id)) {
    s.delete(id)
  } else {
    s.add(id)
  }
  expandedIds.value = s
}

function formatScore(n: number): string {
  return n.toFixed(4)
}

function relativeTime(iso?: string): string {
  if (!iso) return ''
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

function parseTags(tags?: string): string[] {
  if (!tags) return []
  return tags.split(',').map(t => t.trim()).filter(Boolean)
}

function parseSymbols(symbols?: string): string[] {
  if (!symbols) return []
  return symbols.split(',').map(s => s.trim()).filter(Boolean)
}

function truncateBody(body: string, maxLen = 200): string {
  if (body.length <= maxLen) return body
  return body.substring(0, maxLen) + '...'
}
</script>

<template>
  <div class="memory-results">
    <div
      v-for="mem in memories"
      :key="mem.id"
      class="memory-card"
      :class="{ expanded: expandedIds.has(mem.id) }"
      @click="toggleExpand(mem.id)"
    >
      <div class="memory-header">
        <span class="content-type-badge content-type-memory">memory</span>
        <span v-if="mem.timestamp" class="memory-time" :title="mem.timestamp">
          {{ relativeTime(mem.timestamp) }}
        </span>
        <span class="memory-score">{{ formatScore(mem.score) }}</span>
      </div>

      <div class="memory-body">
        {{ expandedIds.has(mem.id) ? mem.body : truncateBody(mem.body) }}
      </div>

      <!-- Tags row -->
      <div v-if="parseTags(mem.tags).length > 0" class="memory-tags">
        <span v-for="tag in parseTags(mem.tags)" :key="tag" class="tag-badge">
          {{ tag }}
        </span>
      </div>

      <!-- Symbols row -->
      <div v-if="parseSymbols(mem.symbols).length > 0" class="memory-symbols">
        <code v-for="sym in parseSymbols(mem.symbols)" :key="sym" class="symbol-chip">
          {{ sym }}
        </code>
      </div>

      <!-- Expanded detail -->
      <div v-if="expandedIds.has(mem.id)" class="memory-detail">
        <div v-if="mem.decision" class="detail-section">
          <span class="detail-label">Decision</span>
          <div class="detail-text">{{ mem.decision }}</div>
        </div>

        <div v-if="mem.impact" class="detail-section">
          <span class="detail-label">Impact</span>
          <div class="detail-text">{{ mem.impact }}</div>
        </div>

        <div v-if="mem.branch" class="detail-section">
          <span class="detail-label">Branch</span>
          <div class="detail-text">
            <span class="pi pi-share-alt detail-icon"></span>
            {{ mem.branch }}
          </div>
        </div>

        <div v-if="mem.file_path" class="detail-section">
          <span class="detail-label">File</span>
          <code class="detail-file">{{ mem.file_path }}</code>
        </div>

        <div class="detail-footer">
          <span class="detail-id">{{ mem.id }}</span>
          <span v-if="mem.timestamp" class="detail-timestamp">{{ mem.timestamp }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.memory-results {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.memory-card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 0.75rem 1rem;
  cursor: pointer;
  transition: border-color 0.15s;
  border-left: 3px solid var(--color-purple);
}

.memory-card:hover {
  border-color: var(--color-purple-border);
}

.memory-card.expanded {
  border-color: var(--color-purple);
}

.memory-header {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.content-type-badge {
  display: inline-block;
  padding: 0.1rem 0.45rem;
  border-radius: 4px;
  font-size: 0.7rem;
  font-weight: 600;
  color: white;
  text-transform: lowercase;
  flex-shrink: 0;
}

.content-type-memory {
  background: var(--color-purple);
}

.memory-time {
  font-size: 0.75rem;
  color: var(--text-secondary);
}

.memory-score {
  margin-left: auto;
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 0.75rem;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.memory-body {
  margin-top: 0.4rem;
  font-size: 0.85rem;
  line-height: 1.5;
  color: var(--text-primary);
  white-space: pre-wrap;
}

.memory-tags {
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
  background: rgba(124, 58, 237, 0.1);
  color: var(--color-purple);
}

.memory-symbols {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
  margin-top: 0.4rem;
}

.symbol-chip {
  padding: 0.15rem 0.45rem;
  border-radius: 4px;
  font-size: 0.75rem;
  background: var(--hover-bg);
  color: var(--color-primary);
  border: 1px solid var(--border-color);
}

/* Expanded detail */
.memory-detail {
  margin-top: 0.75rem;
  padding-top: 0.75rem;
  border-top: 1px solid var(--border-color);
}

.detail-section {
  margin-bottom: 0.6rem;
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
  margin-bottom: 0.2rem;
}

.detail-text {
  font-size: 0.85rem;
  line-height: 1.5;
  color: var(--text-primary);
}

.detail-icon {
  font-size: 0.7rem;
  color: var(--text-secondary);
  margin-right: 0.2rem;
}

.detail-file {
  font-size: 0.75rem;
  font-family: 'SF Mono', 'Fira Code', monospace;
  color: var(--text-secondary);
  padding: 0.15rem 0.4rem;
  background: var(--hover-bg);
  border-radius: 4px;
}

.detail-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 0.6rem;
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
</style>
