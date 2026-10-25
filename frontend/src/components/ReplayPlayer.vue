<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useTemplateRef } from 'vue'

const props = defineProps<{ eventsUrl: string }>()

type Status = 'loading' | 'ready' | 'short' | 'error'
const status = ref<Status>('loading')
const root = useTemplateRef<HTMLElement>('root')
const container = useTemplateRef<HTMLElement>('container')

let player: { $destroy?: () => void } | null = null
let disposed = false

onMounted(async () => {
  try {
    const response = await fetch(props.eventsUrl, { credentials: 'same-origin', headers: { Accept: 'application/json' } })
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    const events: unknown[] = await response.json()
    if (disposed) return
    // rrweb a besoin d'au moins deux événements (méta + snapshot) pour rejouer quoi que ce soit.
    if (!Array.isArray(events) || events.length < 2) {
      status.value = 'short'
      return
    }
    const [{ default: Player }] = await Promise.all([
      import('@sentry-internal/rrweb-player'),
      import('@sentry-internal/rrweb-player/dist/style.css'),
    ])
    if (disposed || !container.value) return
    // On mesure la carte (toujours visible) : l'hôte du lecteur est masqué tant qu'il n'est pas prêt.
    const width = root.value?.clientWidth || 960
    player = new Player({
      target: container.value,
      props: { events: events as never, width, height: Math.round(width * 0.6), autoPlay: false, skipInactive: true, speedOption: [1, 2, 4, 8] },
    })
    status.value = 'ready'
  } catch (e) {
    console.error('chargement du replay', e)
    if (!disposed) status.value = 'error'
  }
})

onBeforeUnmount(() => {
  disposed = true
  player?.$destroy?.()
  player = null
})
</script>

<template>
  <div ref="root" class="replay-player">
    <p v-if="status === 'loading'" class="muted">Chargement du replay…</p>
    <p v-else-if="status === 'short'" class="muted">Replay vide ou trop court pour être rejoué.</p>
    <p v-else-if="status === 'error'" class="error-text">Impossible de charger ce replay.</p>
    <div v-show="status === 'ready'" ref="container" class="player-host"></div>
  </div>
</template>

<style scoped>
.player-host { display: flex; justify-content: center; overflow: hidden; }
</style>
