<script setup lang="ts">
import { Link } from '@inertiajs/vue3'
import Pagination from '../../components/Pagination.vue'
import { levelClass, relativeTime } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { Issue, IssueStatus, ProjectProps } from '../../types'

const props = defineProps<
  ProjectProps & { issues: Issue[]; status: IssueStatus; page: number; pages: number; total: number; counts: Record<IssueStatus, number> }
>()

const tabs: Array<{ status: IssueStatus; label: string }> = [
  { status: 'unresolved', label: 'Non résolues' },
  { status: 'resolved', label: 'Résolues' },
  { status: 'ignored', label: 'Ignorées' },
]

const base = () => `/projects/${props.project.id}/issues`
const pageUrl = (p: number) => `${base()}?status=${props.status}&page=${p}`
</script>

<template>
  <AppLayout :project="project" active="issues">
    <nav class="row tabs">
      <Link v-for="t in tabs" :key="t.status" :href="`${base()}?status=${t.status}`" class="status-tab btn" :class="{ active: t.status === status }">
        {{ t.label }} <span class="muted">{{ counts[t.status] }}</span>
      </Link>
    </nav>
    <div v-if="issues.length === 0" class="card muted">
      Aucune issue ici. Les erreurs envoyées par vos SDK Sentry apparaîtront dans cette liste.
    </div>
    <table v-else class="table card">
      <thead><tr><th>Erreur</th><th>Événements</th><th>Dernier passage</th></tr></thead>
      <tbody>
        <tr v-for="i in issues" :key="i.id">
          <td>
            <span class="level" :class="`level-${levelClass(i.level)}`">{{ i.level }}</span>
            <Link :href="`${base()}/${i.id}`" class="title">{{ i.title }}</Link>
          </td>
          <td>{{ i.event_count }}</td>
          <td class="muted">{{ relativeTime(i.last_seen) }}</td>
        </tr>
      </tbody>
    </table>
    <Pagination :page="page" :pages="pages" :url="pageUrl" />
  </AppLayout>
</template>

<style scoped>
.tabs { margin-bottom: 16px; }
.status-tab.active { background: var(--accent); color: var(--accent-contrast); border-color: transparent; }
.status-tab.active .muted { color: inherit; opacity: .8; }
.title { margin-left: 8px; font-weight: 600; word-break: break-word; }
</style>
