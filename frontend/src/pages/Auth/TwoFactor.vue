<script setup lang="ts">
import { useForm } from '@inertiajs/vue3'
import FormField from '../../components/FormField.vue'
import type { FormErrors } from '../../types'

const props = defineProps<{ challenge: string; errors: FormErrors }>()

const form = useForm({ challenge: props.challenge, code: '' })

function submit() {
  form.post('/login/two-factor', { onFinish: () => form.reset('code') })
}
</script>

<template>
  <div class="center">
    <form class="card stack" @submit.prevent="submit">
      <h1>Vérification en deux étapes</h1>
      <p class="muted">Saisissez le code à 6 chiffres de votre application d’authentification, ou un code de secours.</p>
      <FormField label="Code" :error="errors.code">
        <input v-model="form.code" class="input" name="code" inputmode="numeric" autocomplete="one-time-code" required autofocus />
      </FormField>
      <button class="btn btn-primary" type="submit" :disabled="form.processing">Valider</button>
    </form>
  </div>
</template>

<style scoped>
.center { min-height: 100vh; display: grid; place-items: center; padding: 16px; }
form { width: 100%; max-width: 380px; }
</style>
