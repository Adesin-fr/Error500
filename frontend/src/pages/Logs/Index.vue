<script setup lang="ts">
import { router } from '@inertiajs/vue3'
import { ref } from 'vue'
import { formatDateTime, levelClass, shortId } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { LogEntry, ProjectProps } from '../../types'

const props = defineProps<ProjectProps & { logs: LogEntry[]; filters: { level: string | null; q: string | null } }>()

const level = ref(props.filters.level ?? '')
const q = ref(props.filters.q ?? '')

function search() {
  const params: Record<string, string> = {}
  if (level.value) params.level = level.value
  if (q.value.trim()) params.q = q.value.trim()
  router.get(`/projects/${props.project.id}/logs`, params, { preserveState: true, replace: true })
}

/** Les attributs Sentry sont `{clé: {value, type}}` ; on affiche `clé=valeur`. */
function attributes(l: LogEntry): string {
  if (!l.attributes) return ''
  return Object.entries(l.attributes)
    .map(([k, v]) => `${k}=${v !== null && typeof v === 'object' && 'value' in v ? (v as { value: unknown }).value : v}`)
    .join(' ')
}
</script>

<template>
  <AppLayout :project="project" active="logs">
    <form class="row filters" @submit.prevent="search">
      <select v-model="level" name="level" class="select">
        <option value="">Tous les niveaux</option>
        <option v-for="l in ['trace', 'debug', 'info', 'warn', 'error', 'fatal']" :key="l" :value="l">{{ l }}</option>
      </select>
      <input v-model="q" name="q" class="input grow" type="search" placeholder="Rechercher dans les messages" />
      <button class="btn btn-primary" type="submit">Filtrer</button>
    </form>
    <div v-if="logs.length === 0" class="card muted">Aucun log pour ces critères.</div>
    <div v-else class="card logs">
      <div v-for="l in logs" :key="l.id" class="log-row mono" :class="`level-${levelClass(l.level)}`">
        <span class="muted">{{ formatDateTime(l.occurred_at) }}</span>
        <span class="level" :class="`level-${levelClass(l.level)}`">{{ l.level }}</span>
        <span class="body">
          {{ l.body }}
          <span v-if="attributes(l)" class="muted attrs">{{ attributes(l) }}</span>
          <span v-if="l.trace_id" class="muted attrs">trace:{{ shortId(l.trace_id) }}</span>
        </span>
      </div>
    </div>
  </AppLayout>
</template>

<style scoped>
.filters { margin-bottom: 16px; }
.grow { flex: 1; }
.logs { padding: 0; overflow-x: auto; }
.log-row { display: grid; grid-template-columns: 11em 6em 1fr; gap: 10px; padding: 4px 12px; border-bottom: 1px solid var(--border); align-items: baseline; }
.log-row:last-child { border-bottom: 0; }
.log-row.level-error, .log-row.level-fatal { background: var(--code-line); }
.body { white-space: pre-wrap; word-break: break-word; }
.attrs { margin-left: 8px; }
</style>
