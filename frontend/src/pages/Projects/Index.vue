<script setup lang="ts">
import { Link, useForm } from '@inertiajs/vue3'
import FormField from '../../components/FormField.vue'
import AppLayout from '../../layouts/AppLayout.vue'
import type { FormErrors } from '../../types'

defineProps<{ projects: Array<{ id: number; name: string; slug: string; unresolved_count: number }>; errors: FormErrors }>()

const form = useForm({ name: '' })

function create() {
  form.post('/projects', { onSuccess: () => form.reset() })
}
</script>

<template>
  <AppLayout>
    <h1>Projets</h1>
    <p v-if="projects.length === 0" class="muted">Aucun projet pour l'instant : créez-en un pour obtenir un DSN.</p>
    <div v-else class="grid">
      <Link v-for="p in projects" :key="p.id" :href="`/projects/${p.id}/issues`" class="card project">
        <strong>{{ p.name }}</strong>
        <span :class="p.unresolved_count > 0 ? 'level-error' : 'muted'">
          {{ p.unresolved_count > 0 ? `${p.unresolved_count} non résolues` : 'Aucune erreur non résolue' }}
        </span>
      </Link>
    </div>
    <form class="card new-project" @submit.prevent="create">
      <h2>Nouveau projet</h2>
      <div class="row">
        <FormField label="" :error="errors.name">
          <input v-model="form.name" class="input" name="name" placeholder="Nom du projet" required />
        </FormField>
        <button class="btn btn-primary" type="submit" :disabled="form.processing">Créer</button>
      </div>
    </form>
  </AppLayout>
</template>

<style scoped>
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 12px; margin-bottom: 24px; }
.project { display: grid; gap: 4px; color: var(--text); }
.project:hover { border-color: var(--accent); text-decoration: none; }
.new-project { max-width: 460px; }
</style>
