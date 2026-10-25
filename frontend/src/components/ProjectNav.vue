<script setup lang="ts">
import { Link } from '@inertiajs/vue3'
import type { ProjectRef } from '../types'

export type Section = 'issues' | 'replays' | 'logs' | 'settings'

defineProps<{ project: ProjectRef; active: Section }>()

const sections: Array<{ key: Section; label: string }> = [
  { key: 'issues', label: 'Issues' },
  { key: 'replays', label: 'Replays' },
  { key: 'logs', label: 'Logs' },
  { key: 'settings', label: 'Réglages' },
]
</script>

<template>
  <div class="project-nav">
    <span class="project-name">{{ project.name }}</span>
    <nav class="tabs">
      <Link v-for="s in sections" :key="s.key" :href="`/projects/${project.id}/${s.key}`" class="tab" :class="{ active: s.key === active }">
        {{ s.label }}
      </Link>
    </nav>
  </div>
</template>

<style scoped>
.project-nav { display: flex; gap: 24px; align-items: center; border-bottom: 1px solid var(--border); margin-bottom: 20px; flex-wrap: wrap; }
.project-name { font-size: 18px; font-weight: 700; padding: 8px 0; }
.tabs { display: flex; gap: 4px; }
.tab { padding: 10px 12px; color: var(--muted); border-bottom: 2px solid transparent; margin-bottom: -1px; }
.tab:hover { color: var(--text); text-decoration: none; }
.tab.active { color: var(--text); border-bottom-color: var(--accent); font-weight: 600; }
</style>
