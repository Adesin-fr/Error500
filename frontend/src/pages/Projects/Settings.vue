<script setup lang="ts">
import { router, useForm } from '@inertiajs/vue3'
import { computed } from 'vue'
import CopyField from '../../components/CopyField.vue'
import FormField from '../../components/FormField.vue'
import { formatBytes } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { FormErrors, Member, ProjectProps, SourceMapEntry } from '../../types'

const props = defineProps<
  ProjectProps & {
    dsn: string
    upload_url: string
    upload_token: string | null
    members: Member[]
    /** Préférence d'alertes e-mail de l'utilisateur ; null s'il n'est pas membre du projet. */
    notifications: boolean | null
    retention: { logs_days: number; replays_days: number }
    sourcemaps: SourceMapEntry[]
    errors: FormErrors
  }
>()

const base = computed(() => `/projects/${props.project.id}`)
const opts = { preserveScroll: true }

const memberForm = useForm({ email: '', role: 'member' })

const retentionForm = useForm({ logs_days: props.retention.logs_days, replays_days: props.retention.replays_days })
const deleteForm = useForm({ name: '' })
const canDelete = computed(() => deleteForm.name === props.project.name)

function saveRetention() {
  retentionForm.post(`${base.value}/retention`, opts)
}

function deleteProject() {
  if (canDelete.value) deleteForm.post(`${base.value}/delete`, opts)
}

function addMember() {
  memberForm.post(`${base.value}/members`, { preserveScroll: true, onSuccess: () => memberForm.reset('email') })
}

function confirmThen(message: string, action: () => void) {
  if (window.confirm(message)) action()
}

const setNotifications = (enabled: boolean) => router.post(`${base.value}/notifications`, { enabled }, opts)
const removeMember = (m: Member) =>
  confirmThen(`Retirer ${m.name} du projet ?`, () => router.delete(`${base.value}/members/${m.user_id}`, opts))
const rotateKey = () =>
  confirmThen('Générer une nouvelle clé ? Les SDK configurés avec l’ancien DSN cesseront d’envoyer.', () =>
    router.post(`${base.value}/rotate-key`, {}, opts))
const rotateToken = () =>
  confirmThen('Générer un nouveau jeton d’upload ? L’ancien sera invalidé.', () =>
    router.post(`${base.value}/rotate-upload-token`, {}, opts))
const deleteMap = (m: SourceMapEntry) =>
  confirmThen(`Supprimer ${m.name} ?`, () => router.delete(`${base.value}/sourcemaps/${m.id}`, opts))

const snippet = computed(
  () => `curl -X POST \\
  -H "Authorization: Bearer ${props.upload_token}" \\
  --data-binary @dist/app.min.js.map \\
  "${props.upload_url}?release=1.0.0&name=~%2Fstatic%2Fapp.min.js.map"`,
)
</script>

<template>
  <AppLayout :project="project" active="settings">
    <div class="stack">
      <section class="card stack">
        <h2>Configuration du SDK</h2>
        <CopyField label="DSN" :value="dsn" />
        <div v-if="can_manage"><button type="button" class="btn btn-danger rotate-key" @click="rotateKey">Régénérer la clé</button></div>
      </section>

      <section v-if="notifications !== null" class="card stack">
        <h2>Alertes e-mail</h2>
        <label class="row">
          <input
            class="notifications"
            type="checkbox"
            :checked="notifications"
            @change="setNotifications(($event.target as HTMLInputElement).checked)"
          />
          Recevoir un e-mail pour chaque nouvelle issue et chaque régression de ce projet
        </label>
      </section>

      <section v-if="can_manage" class="card stack">
        <h2>Rétention des données</h2>
        <p class="muted">
          Les logs et les replays plus anciens que ces délais sont supprimés automatiquement (vérification toutes les heures).
          Les erreurs et leurs issues sont conservées. 0 = conserver sans limite.
        </p>
        <form class="row retention" @submit.prevent="saveRetention">
          <FormField label="Logs (jours)" :error="errors.logs_days">
            <input v-model.number="retentionForm.logs_days" class="input" type="number" name="logs_days" min="0" max="3650" required />
          </FormField>
          <FormField label="Replays (jours)" :error="errors.replays_days">
            <input v-model.number="retentionForm.replays_days" class="input" type="number" name="replays_days" min="0" max="3650" required />
          </FormField>
          <button class="btn btn-primary" type="submit" :disabled="retentionForm.processing">Enregistrer</button>
        </form>
      </section>

      <section class="card stack">
        <h2>Membres</h2>
        <table class="table">
          <thead><tr><th>Nom</th><th>E-mail</th><th>Rôle</th><th v-if="can_manage"></th></tr></thead>
          <tbody>
            <tr v-for="m in members" :key="m.user_id">
              <td>{{ m.name }}</td><td>{{ m.email }}</td><td>{{ m.role === 'owner' ? 'Propriétaire' : 'Membre' }}</td>
              <td v-if="can_manage">
                <button type="button" class="btn btn-danger" :class="`remove-member-${m.user_id}`" @click="removeMember(m)">Retirer</button>
              </td>
            </tr>
          </tbody>
        </table>
        <form v-if="can_manage" class="row add-member" @submit.prevent="addMember">
          <FormField label="" :error="errors.email">
            <input v-model="memberForm.email" class="input" type="email" name="email" placeholder="e-mail d'un utilisateur existant" required />
          </FormField>
          <FormField label="" :error="errors.role">
            <select v-model="memberForm.role" class="select" name="role">
              <option value="member">Membre</option>
              <option value="owner">Propriétaire</option>
            </select>
          </FormField>
          <button class="btn btn-primary" type="submit" :disabled="memberForm.processing">Ajouter</button>
        </form>
      </section>

      <section class="card stack">
        <h2>Source maps</h2>
        <template v-if="can_manage && upload_token">
          <p class="muted">
            Envoyez vos fichiers <code>.map</code> après chaque build. Le <code>name</code> est le chemin du fichier minifié
            suivi de <code>.map</code>, sous la forme <code>~/chemin/fichier.js.map</code>. Les maps contenant un
            <code>debugId</code> (plugins Sentry pour Vite/webpack) sont associées automatiquement.
          </p>
          <pre class="snippet mono">{{ snippet }}</pre>
          <CopyField label="Jeton d'upload" :value="upload_token" />
          <div><button type="button" class="btn btn-danger rotate-upload-token" @click="rotateToken">Régénérer le jeton</button></div>
        </template>
        <p v-if="sourcemaps.length === 0" class="muted">Aucune source map envoyée.</p>
        <table v-else class="table">
          <thead><tr><th>Nom</th><th>Release</th><th>Taille</th><th v-if="can_manage"></th></tr></thead>
          <tbody>
            <tr v-for="m in sourcemaps" :key="m.id">
              <td class="mono">{{ m.name }}</td>
              <td>{{ m.release ?? 'toutes' }}</td>
              <td>{{ formatBytes(m.size_bytes) }}</td>
              <td v-if="can_manage">
                <button type="button" class="btn btn-danger" :class="`delete-sourcemap-${m.id}`" @click="deleteMap(m)">Supprimer</button>
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <section v-if="can_manage" class="card stack danger-zone">
        <h2>Supprimer le projet</h2>
        <p class="muted">
          Supprime définitivement le projet, ses issues, événements, logs, replays et source maps. Les SDK qui utilisent son DSN
          recevront des erreurs 403. Cette action est irréversible.
        </p>
        <form class="row delete-project" @submit.prevent="deleteProject">
          <FormField :label="`Saisissez « ${project.name} » pour confirmer`" :error="errors.name">
            <input v-model="deleteForm.name" class="input" type="text" name="name" autocomplete="off" />
          </FormField>
          <button class="btn btn-danger" type="submit" :disabled="!canDelete || deleteForm.processing">Supprimer définitivement</button>
        </form>
      </section>
    </div>
  </AppLayout>
</template>

<style scoped>
.snippet { margin: 0; padding: 12px; background: var(--code-bg); border-radius: 6px; overflow-x: auto; }
</style>
