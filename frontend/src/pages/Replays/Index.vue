<script setup lang="ts">
import { Link, router } from '@inertiajs/vue3'
import { ref } from 'vue'
import { formatDuration, relativeTime, shortId } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { ProjectProps, Replay, ReplayUser } from '../../types'

const props = defineProps<ProjectProps & { replays: Replay[]; filters: { user: string | null }; users: ReplayUser[] }>()

const userInput = ref(props.filters.user ?? '')

/** Recharge la liste côté serveur ; le champ garde sa valeur pendant la navigation. */
function visit(user: string) {
  const query = user ? { user } : {}
  router.get(`/projects/${props.project.id}/replays`, query, { preserveState: true, replace: true })
}
const applyFilter = () => visit(userInput.value.trim())
function resetFilter() {
  userInput.value = ''
  visit('')
}

const firstUrl = (r: Replay) => r.urls[0]?.replace(/^https?:\/\//, '') ?? '—'
</script>

<template>
  <AppLayout :project="project" active="replays">
    <form v-if="replays.length > 0 || filters.user" class="user-filter row" @submit.prevent="applyFilter">
      <label class="muted" for="replay-user">Utilisateur</label>
      <input
        id="replay-user"
        v-model="userInput"
        class="input"
        type="search"
        name="user"
        list="replay-users"
        placeholder="e-mail ou identifiant"
        autocomplete="off"
      />
      <datalist id="replay-users">
        <option v-for="u in users" :key="u.label" :value="u.label">{{ u.count }} {{ u.count > 1 ? 'sessions' : 'session' }}</option>
      </datalist>
      <button class="btn btn-primary" type="submit">Filtrer</button>
      <button v-if="filters.user" class="btn reset-filter" type="button" @click="resetFilter">Réinitialiser</button>
    </form>
    <div v-if="replays.length === 0 && filters.user" class="card muted">Aucun replay pour « {{ filters.user }} ».</div>
    <div v-else-if="replays.length === 0" class="card muted">
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
.user-filter { margin-bottom: 12px; align-items: center; }
.user-filter .input { min-width: 280px; }
.live-badge { margin-left: 8px; padding: 1px 8px; border-radius: 999px; font-size: 12px; font-weight: 600; color: #fff; background: var(--danger, #c0392b); }
.url { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
