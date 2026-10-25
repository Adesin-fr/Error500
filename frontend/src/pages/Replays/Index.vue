<script setup lang="ts">
import { Link } from '@inertiajs/vue3'
import { formatDuration, relativeTime, shortId } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { ProjectProps, Replay } from '../../types'

defineProps<ProjectProps & { replays: Replay[] }>()

const firstUrl = (r: Replay) => r.urls[0]?.replace(/^https?:\/\//, '') ?? '—'
</script>

<template>
  <AppLayout :project="project" active="replays">
    <div v-if="replays.length === 0" class="card muted">
      Aucun replay. Activez <code>replayIntegration()</code> dans le SDK navigateur Sentry : les sessions apparaîtront ici.
    </div>
    <table v-else class="table card">
      <thead><tr><th>Session</th><th>Utilisateur</th><th>Durée</th><th>Première page</th><th>Navigateur</th><th>Date</th></tr></thead>
      <tbody>
        <tr v-for="r in replays" :key="r.replay_id">
          <td><Link :href="`/projects/${project.id}/replays/${r.replay_id}`" class="mono">▶ {{ shortId(r.replay_id) }}</Link>
            <span v-if="r.live" class="live-badge">En direct</span>
          </td>
          <td>{{ r.user_label ?? 'anonyme' }}</td>
          <td>{{ formatDuration(r.duration_seconds) }}</td>
          <td class="mono url">{{ firstUrl(r) }}</td>
          <td>{{ r.browser ?? '—' }}</td>
          <td class="muted">{{ relativeTime(r.last_seen) }}</td>
        </tr>
      </tbody>
    </table>
  </AppLayout>
</template>

<style scoped>
.live-badge { margin-left: 8px; padding: 1px 8px; border-radius: 999px; font-size: 12px; font-weight: 600; color: #fff; background: var(--danger, #c0392b); }
.url { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
