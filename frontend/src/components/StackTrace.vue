<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ExceptionView, Frame } from '../types'
import StackFrame from './StackFrame.vue'

const props = defineProps<{ exceptions: ExceptionView[] }>()

const onlyApp = ref(false)

function title(e: ExceptionView): string {
  const type = e.type ?? 'Error'
  return e.value ? `${type}: ${e.value}` : type
}

const visible = computed(() =>
  props.exceptions.map((e) => ({
    title: title(e),
    total: e.frames.length,
    frames: onlyApp.value ? e.frames.filter((f: Frame) => f.in_app) : e.frames,
  })),
)

// Seul le premier frame applicatif de la première exception est déplié d'emblée.
const firstOpen = computed(() => {
  const frames = visible.value[0]?.frames ?? []
  return frames.findIndex((f) => f.in_app && f.context_line !== null)
})
</script>

<template>
  <section class="stack-trace">
    <p v-if="exceptions.length === 0" class="muted">Aucune stack trace pour cet événement.</p>
    <template v-else>
      <label class="toggle">
        <input v-model="onlyApp" type="checkbox" /> Mon code uniquement
      </label>
      <div v-for="(e, ei) in visible" :key="ei" class="exception">
        <h3 class="exception-title">{{ e.title }}</h3>
        <p v-if="e.total === 0" class="muted">Pas de frames.</p>
        <p v-else-if="e.frames.length === 0" class="muted">Aucun frame de l'application.</p>
        <ul v-else class="frames">
          <StackFrame v-for="(f, fi) in e.frames" :key="fi" :frame="f" :default-open="ei === 0 && fi === firstOpen" />
        </ul>
      </div>
    </template>
  </section>
</template>

<style scoped>
.toggle { display: inline-flex; gap: 6px; align-items: center; margin-bottom: 8px; color: var(--muted); }
.exception { margin-bottom: 16px; }
.exception-title { margin: 0 0 4px; font-size: 15px; color: var(--danger); word-break: break-word; }
.frames { margin: 0; padding: 0; }
</style>
