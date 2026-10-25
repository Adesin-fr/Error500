<script setup lang="ts">
import { Link, router } from '@inertiajs/vue3'
import { computed, onBeforeUnmount, onMounted, ref, useTemplateRef } from 'vue'
import Breadcrumbs from '../../components/Breadcrumbs.vue'
import ReplayPlayer from '../../components/ReplayPlayer.vue'
import StackTrace from '../../components/StackTrace.vue'
import { formatDateTime, levelClass, relativeTime } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { EventDetail, EventSummary, ExceptionView, Issue, IssueStatus, ProjectProps, ReplayTimeline } from '../../types'

const props = defineProps<
  ProjectProps & {
    issue: Issue
    event: EventDetail
    exceptions: ExceptionView[]
    events: EventSummary[]
    replay_id: string | null
    replay_timeline: ReplayTimeline | null
  }
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
const markers = computed(() =>
  (props.replay_timeline?.markers ?? []).map((m) => ({ timestamp: Date.parse(m.occurred_at), label: m.title, current: m.current })),
)

// Le replay pèse plusieurs Mo : on ne le télécharge que quand le pied de page entre dans l'écran.
const footer = useTemplateRef<HTMLElement>('footer')
const showReplay = ref(false)
let observer: IntersectionObserver | null = null
onMounted(() => {
  if (!props.replay_timeline) return
  if (typeof IntersectionObserver === 'undefined' || !footer.value) {
    showReplay.value = true
    return
  }
  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        showReplay.value = true
        observer?.disconnect()
      }
    },
    { rootMargin: '300px' },
  )
  observer.observe(footer.value)
})
onBeforeUnmount(() => observer?.disconnect())

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

    <section v-if="replay_timeline" ref="footer" class="card stack replay-footer">
      <div class="row between">
        <h2>Replay de la session</h2>
        <Link :href="`/projects/${project.id}/replays/${replay_id}`">▶ Ouvrir la session complète</Link>
      </div>
      <p class="muted legend">
        Repères sur la timeline : <span class="swatch error"></span>Cette erreur <span class="swatch other"></span>Autres erreurs de la session
      </p>
      <ReplayPlayer v-if="showReplay" :events-url="replay_timeline.events_url" :markers="markers" />
      <p v-else class="muted">Le replay se charge à l’arrivée en bas de page…</p>
    </section>
  </AppLayout>
</template>

<style scoped>
.replay-footer { margin-top: 16px; }
.between { justify-content: space-between; align-items: center; }
.legend { margin: 0; }
.swatch { display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin: 0 4px 0 12px; }
.swatch.error { background: #dc2626; }
.swatch.other { background: #f59e0b; }
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
