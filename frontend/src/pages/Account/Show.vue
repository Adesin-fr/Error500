<script setup lang="ts">
import { router, useForm } from '@inertiajs/vue3'
import FormField from '../../components/FormField.vue'
import AppLayout from '../../layouts/AppLayout.vue'
import type { FormErrors, TwoFactorState } from '../../types'

defineProps<{ two_factor: TwoFactorState; recovery_codes: string[] | null; errors: FormErrors }>()

const confirmForm = useForm({ code: '' })
const disableForm = useForm({ password: '' })

const startSetup = () => router.post('/account/2fa/setup', {}, { preserveScroll: true })
const confirm = () => confirmForm.post('/account/2fa/enable', { preserveScroll: true, onFinish: () => confirmForm.reset('code') })
const disable = () => disableForm.post('/account/2fa/disable', { preserveScroll: true, onFinish: () => disableForm.reset('password') })
</script>

<template>
  <AppLayout>
    <h1>Mon compte</h1>
    <section class="card stack">
      <h2>Authentification à deux facteurs</h2>

      <template v-if="recovery_codes">
        <p><strong>L’authentification à deux facteurs est active.</strong> Conservez ces codes de secours : chacun ne sert qu’une fois et ils ne seront plus affichés.</p>
        <ul class="codes">
          <li v-for="c in recovery_codes" :key="c"><code>{{ c }}</code></li>
        </ul>
      </template>

      <template v-if="two_factor.enabled">
        <p>Active. Codes de secours restants : {{ two_factor.recovery_remaining }}.</p>
        <form class="stack disable-2fa" @submit.prevent="disable">
          <FormField label="Mot de passe pour désactiver" :error="errors.password">
            <input v-model="disableForm.password" class="input" type="password" name="password" autocomplete="current-password" required />
          </FormField>
          <div><button class="btn btn-danger" type="submit" :disabled="disableForm.processing">Désactiver</button></div>
        </form>
      </template>

      <template v-else-if="two_factor.secret">
        <p>Scannez ce QR code avec Google Authenticator (ou toute application TOTP), puis saisissez le code affiché.</p>
        <!-- eslint-disable-next-line vue/no-v-html -- SVG produit par notre serveur, sans contenu utilisateur -->
        <div class="qr" v-html="two_factor.qr_svg"></div>
        <p class="muted">Saisie manuelle : <code>{{ two_factor.secret }}</code></p>
        <form class="stack confirm-2fa" @submit.prevent="confirm">
          <FormField label="Code à 6 chiffres" :error="errors.code">
            <input v-model="confirmForm.code" class="input" name="code" inputmode="numeric" autocomplete="one-time-code" required />
          </FormField>
          <div><button class="btn btn-primary" type="submit" :disabled="confirmForm.processing">Activer</button></div>
        </form>
      </template>

      <p v-else-if="!two_factor.available" class="muted">
        Indisponible : définissez <code>SECRET_KEY</code> dans la configuration du serveur (<code>openssl rand -hex 32</code>), puis redémarrez.
      </p>

      <template v-else>
        <span v-if="errors.setup" class="error-text">{{ errors.setup }}</span>
        <p class="muted">Désactivée. Un code de votre téléphone sera demandé en plus du mot de passe.</p>
        <div><button type="button" class="btn btn-primary enable-2fa" @click="startSetup">Activer</button></div>
      </template>
    </section>
  </AppLayout>
</template>

<style scoped>
.codes { list-style: none; padding: 0; display: grid; grid-template-columns: repeat(2, max-content); gap: 8px 32px; }
.qr { width: 200px; background: #fff; padding: 8px; }
.qr :deep(svg) { width: 100%; height: auto; display: block; }
</style>
