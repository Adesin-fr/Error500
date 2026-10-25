<script setup lang="ts">
import { useForm } from '@inertiajs/vue3'
import FormField from '../../components/FormField.vue'
import type { FormErrors } from '../../types'

defineProps<{ errors: FormErrors }>()

const form = useForm({ email: '', password: '' })

function submit() {
  form.post('/login', { onFinish: () => form.reset('password') })
}
</script>

<template>
  <div class="center">
    <form class="card stack" @submit.prevent="submit">
      <h1>Connexion à error500</h1>
      <FormField label="Adresse e-mail" :error="errors.email">
        <input v-model="form.email" class="input" type="email" name="email" autocomplete="username" required autofocus />
      </FormField>
      <FormField label="Mot de passe">
        <input v-model="form.password" class="input" type="password" name="password" autocomplete="current-password" required />
      </FormField>
      <button class="btn btn-primary" type="submit" :disabled="form.processing">Se connecter</button>
    </form>
  </div>
</template>

<style scoped>
.center { min-height: 100vh; display: grid; place-items: center; padding: 16px; }
form { width: 100%; max-width: 380px; }
</style>
