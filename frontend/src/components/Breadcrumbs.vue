<script setup lang="ts">
import { formatDateTime, levelClass } from '../lib/format'
import type { Breadcrumb } from '../types'

defineProps<{ breadcrumbs: Breadcrumb[] }>()

function time(b: Breadcrumb): string {
  if (b.timestamp === undefined) return ''
  const date = typeof b.timestamp === 'number' ? new Date(b.timestamp * 1000) : new Date(b.timestamp)
  return Number.isNaN(date.getTime()) ? '' : formatDateTime(date.toISOString())
}
</script>

<template>
  <p v-if="breadcrumbs.length === 0" class="muted">Aucun breadcrumb.</p>
  <ol v-else class="breadcrumbs">
    <li v-for="(b, i) in breadcrumbs" :key="i" class="breadcrumb" :class="`level-${levelClass(b.level ?? 'info')}`">
      <span class="time mono muted">{{ time(b) }}</span>
      <span class="category">{{ b.category ?? b.type ?? '' }}</span>
      <span class="message">{{ b.message ?? '' }}</span>
    </li>
  </ol>
</template>

<style scoped>
.breadcrumbs { margin: 0; padding: 0; list-style: none; }
.breadcrumb { display: grid; grid-template-columns: 11em 9em 1fr; gap: 8px; padding: 4px 0; border-bottom: 1px solid var(--border); }
.category { font-weight: 600; }
.level-error .message, .level-fatal .message { color: var(--danger); }
.level-warning .message { color: var(--warning); }
.message { word-break: break-word; }
</style>
