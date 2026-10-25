<script setup lang="ts">
import { Link, router } from '@inertiajs/vue3'
import { computed } from 'vue'
import Breadcrumbs from '../../components/Breadcrumbs.vue'
import StackTrace from '../../components/StackTrace.vue'
import { formatDateTime, levelClass, relativeTime } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { EventDetail, EventSummary, ExceptionView, Issue, IssueStatus, ProjectProps } from '../../types'

const props = defineProps<
  ProjectProps & { issue: Issue; event: EventDetail; exceptions: ExceptionView[]; events: EventSummary[]; replay_id: string | null }
>()

const issueUrl = computed(() => `/projects/${props.project.id}/issues/${props.issue.id}`)

function setStatus(status: IssueStatus) {
  router.post(`${issueUrl.value}/status`, { status }, { preserveScroll: true })
}

const contexts = computed(() =>
  Object.entries(props.event.contexts)
    .map(([key, c]) => ({ key, label: [c.name ?? c.family ?? c.model ?? c.brand, c.version].filter(Boolean).join(' ') }))
    .filter((c) => c.label),
)
const userLabel = computed(() => props.event.user.email ?? props.event.user.username ?? props.event.user.id ?? props.event.user.ip_address ?? null)
</script>

<template>
  <AppLayout :project="project" active="issues">
    <header class="stack head">
      <div>
        <span class="level" :class="`level-${levelClass(issue.level)}`">{{ issue.level }}</span>
        <h1 class="issue-title">{{ issue.title }}</h1>
        <p class="muted">
          {{ issue.event_count }} événements · vu pour la première fois {{ relativeTime(issue.first_seen) }}
          · dernier passage {{ relativeTime(issue.last_seen) }}
        </p>
      </div>
      <div class="row">
        <button v-if="issue.status !== 'resolved'" class="btn btn-primary set-resolved" type="button" @click="setStatus('resolved')">Résoudre</button>
        <button v-if="issue.status !== 'ignored'" class="btn set-ignored" type="button" @click="setStatus('ignored')">Ignorer</button>
        <button v-if="issue.status !== 'unresolved'" class="btn set-unresolved" type="button" @click="setStatus('unresolved')">Rouvrir</button>
        <Link v-if="replay_id" class="btn replay-link" :href="`/projects/${project.id}/replays/${replay_id}`">▶ Voir la session</Link>
      </div>
    </header>

    <div class="layout">
      <div class="stack">
        <section class="card">
          <h2>Stack trace</h2>
          <p v-if="event.message && exceptions.length === 0" class="mono">{{ event.message }}</p>
          <StackTrace :exceptions="exceptions" />
        </section>
        <section class="card">
          <h2>Breadcrumbs</h2>
          <Breadcrumbs :breadcrumbs="event.breadcrumbs" />
        </section>
      </div>

      <aside class="stack">
        <section class="card">
          <h2>Événement</h2>
          <dl class="meta">
            <dt>Date</dt><dd>{{ formatDateTime(event.occurred_at) }}</dd>
            <dt>ID</dt><dd class="mono">{{ event.event_id }}</dd>
            <template v-if="event.release"><dt>Release</dt><dd>{{ event.release }}</dd></template>
            <template v-if="event.environment"><dt>Environnement</dt><dd>{{ event.environment }}</dd></template>
            <template v-if="userLabel"><dt>Utilisateur</dt><dd>{{ userLabel }}</dd></template>
            <template v-if="event.request.url"><dt>URL</dt><dd class="mono wrap">{{ event.request.method }} {{ event.request.url }}</dd></template>
            <template v-for="c in contexts" :key="c.key"><dt>{{ c.key }}</dt><dd>{{ c.label }}</dd></template>
          </dl>
        </section>
        <section v-if="event.tags.length" class="card">
          <h2>Tags</h2>
          <dl class="meta">
            <template v-for="t in event.tags" :key="t.key"><dt>{{ t.key }}</dt><dd class="wrap">{{ t.value }}</dd></template>
          </dl>
        </section>
        <section class="card">
          <h2>Occurrences</h2>
          <ul class="occurrences">
            <li v-for="e in events" :key="e.event_id">
              <Link :href="`${issueUrl}?event=${e.event_id}`" :class="{ current: e.event_id === event.event_id }">
                {{ formatDateTime(e.occurred_at) }}
              </Link>
            </li>
          </ul>
        </section>
      </aside>
    </div>
  </AppLayout>
</template>

<style scoped>
.head { margin-bottom: 16px; }
.issue-title { margin: 4px 0; word-break: break-word; }
.layout { display: grid; grid-template-columns: minmax(0, 1fr) 320px; gap: 16px; align-items: start; }
@media (max-width: 900px) { .layout { grid-template-columns: 1fr; } }
.meta { display: grid; grid-template-columns: max-content 1fr; gap: 4px 12px; margin: 0; }
.meta dt { color: var(--muted); }
.meta dd { margin: 0; }
.wrap { word-break: break-all; }
.occurrences { margin: 0; padding: 0; list-style: none; }
.occurrences .current { font-weight: 700; }
</style>
