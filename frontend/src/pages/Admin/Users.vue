<script setup lang="ts">
import { useForm } from '@inertiajs/vue3'
import FormField from '../../components/FormField.vue'
import { formatDateTime } from '../../lib/format'
import AppLayout from '../../layouts/AppLayout.vue'
import type { FormErrors, ManagedUser } from '../../types'

defineProps<{ users: ManagedUser[]; errors: FormErrors }>()

const form = useForm({ name: '', email: '', password: '', is_admin: false })

function create() {
  form.post('/admin/users', { onSuccess: () => form.reset() })
}
</script>

<template>
  <AppLayout>
    <h1>Utilisateurs</h1>
    <table class="table card">
      <thead><tr><th>Nom</th><th>E-mail</th><th>Rôle</th><th>Créé le</th></tr></thead>
      <tbody>
        <tr v-for="u in users" :key="u.id">
          <td>{{ u.name }}</td><td>{{ u.email }}</td>
          <td>{{ u.is_admin ? 'Admin' : 'Utilisateur' }}</td>
          <td class="muted">{{ formatDateTime(u.created_at) }}</td>
        </tr>
      </tbody>
    </table>
    <form class="card stack new-user" @submit.prevent="create">
      <h2>Nouvel utilisateur</h2>
      <FormField label="Nom" :error="errors.name"><input v-model="form.name" class="input" name="name" required /></FormField>
      <FormField label="Adresse e-mail" :error="errors.email"><input v-model="form.email" class="input" type="email" name="email" required /></FormField>
      <FormField label="Mot de passe" :error="errors.password"><input v-model="form.password" class="input" type="password" name="password" autocomplete="new-password" required /></FormField>
      <label class="row"><input v-model="form.is_admin" type="checkbox" name="is_admin" /> Administrateur</label>
      <div><button class="btn btn-primary" type="submit" :disabled="form.processing">Créer</button></div>
    </form>
  </AppLayout>
</template>

<style scoped>
.new-user { max-width: 420px; margin-top: 24px; }
</style>
