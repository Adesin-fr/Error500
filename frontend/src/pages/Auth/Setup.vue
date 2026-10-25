<script setup lang="ts">
import { useForm } from '@inertiajs/vue3'
import FormField from '../../components/FormField.vue'
import type { FormErrors } from '../../types'

defineProps<{ errors: FormErrors }>()

const form = useForm({ name: '', email: '', password: '' })

function submit() {
  form.post('/setup', { onFinish: () => form.reset('password') })
}
</script>

<template>
  <div class="center">
    <form class="card stack" @submit.prevent="submit">
      <h1>Bienvenue sur error500</h1>
      <p class="muted">Créez le compte administrateur pour commencer.</p>
      <FormField label="Nom" :error="errors.name">
        <input v-model="form.name" class="input" name="name" autocomplete="name" required autofocus />
      </FormField>
      <FormField label="Adresse e-mail" :error="errors.email">
        <input v-model="form.email" class="input" type="email" name="email" autocomplete="username" required />
      </FormField>
      <FormField label="Mot de passe (8 caractères minimum)" :error="errors.password">
        <input v-model="form.password" class="input" type="password" name="password" autocomplete="new-password" required />
      </FormField>
      <button class="btn btn-primary" type="submit" :disabled="form.processing">Créer le compte</button>
    </form>
  </div>
</template>

<style scoped>
.center { min-height: 100vh; display: grid; place-items: center; padding: 16px; }
form { width: 100%; max-width: 420px; }
</style>
