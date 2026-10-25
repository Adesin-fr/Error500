<script setup lang="ts">
import { Link, router, usePage } from '@inertiajs/vue3'
import { computed } from 'vue'
import ProjectNav, { type Section } from '../components/ProjectNav.vue'
import type { ProjectRef, SharedProps } from '../types'

defineProps<{ project?: ProjectRef; active?: Section }>()

const page = usePage<SharedProps>()
const user = computed(() => page.props.auth.user)

function logout() {
  router.post('/logout')
}
</script>

<template>
  <div class="app">
    <header class="topbar">
      <Link href="/projects" class="brand">error500</Link>
      <Link href="/projects" class="nav-link">Projets</Link>
      <Link v-if="user?.is_admin" href="/admin/users" class="nav-link">Utilisateurs</Link>
      <span class="spacer"></span>
      <span v-if="user" class="muted">{{ user.name }}</span>
      <button v-if="user" type="button" class="btn" @click="logout">Déconnexion</button>
    </header>
    <main class="content">
      <ProjectNav v-if="project && active" :project="project" :active="active" />
      <slot />
    </main>
  </div>
</template>

<style scoped>
.topbar { display: flex; gap: 16px; align-items: center; padding: 10px 24px; background: var(--surface); border-bottom: 1px solid var(--border); }
.brand { font-weight: 800; letter-spacing: -.02em; color: var(--text); }
.nav-link { color: var(--muted); }
.content { max-width: 1100px; margin: 0 auto; padding: 24px; }
</style>
