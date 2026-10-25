<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, useTemplateRef } from 'vue'
import { normalizeReplayEvents, withMarkers, type ReplayMarker } from '../lib/replay'

const props = defineProps<{
  eventsUrl: string
  /** Flux SSE des segments (session en cours). */
  streamUrl?: string
  /** Suivre la session en direct plutôt que de charger un enregistrement terminé. */
  live?: boolean
  /** Erreurs à repérer sur la timeline (replay enregistré uniquement). */
  markers?: ReplayMarker[]
}>()

type Status = 'loading' | 'ready' | 'short' | 'error'
const status = ref<Status>('loading')
/** État de la session annoncé par le serveur : active (segments récents) ou inactive. */
const sessionLive = ref<boolean | null>(null)
const disconnected = ref(false)
const root = useTemplateRef<HTMLElement>('root')
const container = useTemplateRef<HTMLElement>('container')

interface PlayerHandle {
  $destroy?: () => void
  addEvent?: (event: unknown) => void
  getReplayer?: () => { startLive: (baselineTime?: number) => void }
  setSpeed?: (speed: number) => void
  goto?: (timeOffset: number, play?: boolean) => void
}
let player: PlayerHandle | null = null
let source: EventSource | null = null
let disposed = false

async function createPlayer(events: unknown[], extra: Record<string, unknown> = {}): Promise<PlayerHandle | null> {
  const [{ default: Player }] = await Promise.all([
    import('@sentry-internal/rrweb-player'),
    import('@sentry-internal/rrweb-player/dist/style.css'),
  ])
  if (disposed || !container.value) return null
  // On mesure la carte (toujours visible) : l'hôte du lecteur est masqué tant qu'il n'est pas prêt.
  const width = root.value?.clientWidth || 960
  return new Player({
    target: container.value,
    props: { events: normalizeReplayEvents(events) as never, width, height: Math.round(width * 0.6), autoPlay: false, skipInactive: true, speedOption: [1, 2, 4, 8], ...extra },
  }) as unknown as PlayerHandle
}

async function loadRecording() {
  const response = await fetch(props.eventsUrl, { credentials: 'same-origin', headers: { Accept: 'application/json' } })
  if (!response.ok) throw new Error(`HTTP ${response.status}`)
  const events: unknown[] = await response.json()
  if (disposed) return
  // rrweb a besoin d'au moins deux événements (méta + snapshot) pour rejouer quoi que ce soit.
  if (!Array.isArray(events) || events.length < 2) {
    status.value = 'short'
    return
  }
  const marked = props.markers?.length ? withMarkers(normalizeReplayEvents(events), props.markers) : null
  player = await createPlayer(marked?.events ?? events, marked ? { tags: marked.tags } : {})
  if (!player) return
  if (marked?.seekMs != null) player.goto?.(marked.seekMs)
  status.value = 'ready'
}

// --- Mode live : les segments arrivent un à un, on les applique dans l'ordre de réception. ---

/** Au-delà de cet historique, on rattrape en accéléré avant de passer à la vitesse normale. */
const CATCH_UP_THRESHOLD_MS = 3000
const CATCH_UP_SPEED = 8
const CATCH_UP_MARGIN_MS = 1000

interface SegmentMessage { segment_id: number; events: unknown[] }
const seen = new Set<number>()
let queue: Promise<void> = Promise.resolve()

async function applySegment({ segment_id, events }: SegmentMessage) {
  if (seen.has(segment_id) || disposed) return
  seen.add(segment_id)
  const normalized = normalizeReplayEvents(events) as Array<{ timestamp?: number }>
  if (!player) {
    // Baseline = plus ancien timestamp : rrweb applique « en rattrapage » (sans mutations du DOM, donc
    // écran blanc) tout événement antérieur à la baseline. Tout passe donc par la minuterie, et on
    // accélère le temps de rattraper l'historique déjà enregistré.
    const stamps = normalized.map((e) => e.timestamp ?? 0)
    const baseline = Math.min(...stamps)
    const history = Math.max(...stamps) - baseline
    const catchUp = history > CATCH_UP_THRESHOLD_MS
    player = await createPlayer(normalized, { liveMode: true, ...(catchUp ? { speed: CATCH_UP_SPEED } : {}) })
    if (!player) return
    player.getReplayer?.().startLive(baseline)
    status.value = 'ready'
    if (catchUp) {
      const handle = player
      setTimeout(() => {
        if (!disposed) handle.setSpeed?.(1)
      }, history / CATCH_UP_SPEED + CATCH_UP_MARGIN_MS)
    }
  }
  for (const event of normalized) player.addEvent?.(event)
}

function followLive() {
  source = new EventSource(props.streamUrl!)
  const parse = <T,>(raw: { data: string }) => {
    disconnected.value = false
    return JSON.parse(raw.data) as T
  }
  source.addEventListener('status', (e) => {
    sessionLive.value = parse<{ live: boolean }>(e as MessageEvent).live
  })
  source.addEventListener('segment', (e) => {
    const segment = parse<SegmentMessage>(e as MessageEvent)
    queue = queue.then(() => applySegment(segment)).catch((err) => console.error('segment de replay', err))
  })
  source.onerror = () => {
    disconnected.value = true
  }
}

onMounted(async () => {
  try {
    if (props.live && props.streamUrl) followLive()
    else await loadRecording()
  } catch (e) {
    console.error('chargement du replay', e)
    if (!disposed) status.value = 'error'
  }
})

onBeforeUnmount(() => {
  disposed = true
  source?.close()
  source = null
  player?.$destroy?.()
  player = null
})
</script>

<template>
  <div ref="root" class="replay-player">
    <p v-if="live && streamUrl" class="live-status" :class="{ quiet: sessionLive === false }">
      <template v-if="disconnected">Connexion interrompue, reconnexion…</template>
      <template v-else-if="sessionLive === false">Session inactive, en attente d’activité</template>
      <template v-else><span class="dot"></span>En direct</template>
    </p>
    <p v-if="status === 'loading' && live" class="muted">En attente des premières données…</p>
    <p v-else-if="status === 'loading'" class="muted">Chargement du replay…</p>
    <p v-else-if="status === 'short'" class="muted">Replay vide ou trop court pour être rejoué.</p>
    <p v-else-if="status === 'error'" class="error-text">Impossible de charger ce replay.</p>
    <div v-show="status === 'ready'" ref="container" class="player-host"></div>
  </div>
</template>

<style scoped>
.live-status { display: flex; align-items: center; gap: 6px; margin: 0 0 8px; font-weight: 600; color: var(--danger, #c0392b); }
.live-status.quiet { color: var(--muted); font-weight: 400; }
.dot { width: 8px; height: 8px; border-radius: 50%; background: currentColor; animation: pulse 1.4s ease-in-out infinite; }
@keyframes pulse { 50% { opacity: 0.25; } }
.player-host { display: flex; justify-content: center; overflow: hidden; }
</style>
