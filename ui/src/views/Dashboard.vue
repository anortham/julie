<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'

interface HealthData {
  status: string
  version: string
  uptime_seconds: number
}

const health = ref<HealthData | null>(null)
const error = ref<string | null>(null)
const loading = ref(true)

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

async function fetchHealth() {
  try {
    const res = await fetch('/api/health')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    health.value = await res.json()
    error.value = null
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch health'
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  fetchHealth()
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

    <div v-else class="cards">
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
  border-color: #fca5a5;
  color: #dc2626;
  background: #fef2f2;
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 1rem;
}

.card {
  background: var(--card-bg);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 1.25rem;
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

.card-value {
  font-size: 1.5rem;
  font-weight: 700;
}

.status-ok {
  color: #16a34a;
}
</style>
