<script setup lang="ts">
import { Link } from '@inertiajs/vue3'
import ReplayPlayer from '../../components/ReplayPlayer.vue'
import { formatDateTime, formatDuration, shortId } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { LinkedError, ProjectProps, Replay } from '../../types'

defineProps<ProjectProps & { replay: Replay; events_url: string; stream_url: string; errors: LinkedError[] }>()
</script>

<template>
  <AppLayout :project="project" active="replays">
    <h1>Session <span class="mono">{{ shortId(replay.replay_id) }}</span></h1>
    <div class="layout">
      <section class="card">
        <ReplayPlayer :events-url="events_url" :stream-url="stream_url" :live="replay.live" />
      </section>
      <aside class="stack">
        <section class="card">
          <h2>Session</h2>
          <dl class="meta">
            <dt>Début</dt><dd>{{ formatDateTime(replay.first_seen) }}</dd>
            <dt>Durée</dt><dd>{{ formatDuration(replay.duration_seconds) }}</dd>
            <dt>Utilisateur</dt><dd>{{ replay.user_label ?? 'anonyme' }}</dd>
            <dt>Navigateur</dt><dd>{{ replay.browser ?? '—' }}</dd>
            <dt>Système</dt><dd>{{ replay.os ?? '—' }}</dd>
            <template v-if="replay.release_name"><dt>Release</dt><dd>{{ replay.release_name }}</dd></template>
            <template v-if="replay.environment"><dt>Environnement</dt><dd>{{ replay.environment }}</dd></template>
          </dl>
        </section>
        <section v-if="replay.urls.length" class="card">
          <h2>Pages visitées</h2>
          <ol class="urls"><li v-for="(u, i) in replay.urls" :key="i" class="mono">{{ u }}</li></ol>
        </section>
        <section class="card">
          <h2>Erreurs pendant la session</h2>
          <p v-if="errors.length === 0" class="muted">Aucune erreur liée.</p>
          <ul v-else class="errors">
            <li v-for="e in errors" :key="e.event_id">
              <Link :href="`/projects/${project.id}/issues/${e.issue_id}?event=${e.event_id}`">{{ e.title }}</Link>
            </li>
          </ul>
        </section>
      </aside>
    </div>
  </AppLayout>
</template>

<style scoped>
.layout { display: grid; grid-template-columns: minmax(0, 1fr) 300px; gap: 16px; align-items: start; }
@media (max-width: 900px) { .layout { grid-template-columns: 1fr; } }
.meta { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 0; }
.meta dt { color: var(--muted); }
.meta dd { margin: 0; }
.urls, .errors { margin: 0; padding-left: 18px; }
.urls li { word-break: break-all; }
</style>
