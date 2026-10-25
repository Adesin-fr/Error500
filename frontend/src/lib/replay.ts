/** rrweb : type des événements personnalisés (breadcrumbs, performanceSpan… ajoutés par le SDK Sentry). */
const CUSTOM_EVENT = 5
/** En dessous, un timestamp ne peut être qu'en secondes (1e11 ms = 1973, 1e11 s = an 5138). */
const MAX_SECONDS_TIMESTAMP = 1e11

/**
 * Le SDK Sentry horodate ses événements personnalisés en secondes (flottantes) alors que rrweb attend des
 * millisecondes. Mélangés, ils étirent la durée apparente de la session de plusieurs décennies et
 * se retrouvent avant le premier snapshot. Renvoie une copie ; l'entrée n'est pas modifiée.
 */
export function normalizeReplayEvents(events: unknown[]): unknown[] {
  return events.map((event) => {
    if (typeof event !== 'object' || event === null) return event
    const e = event as { type?: unknown; timestamp?: unknown }
    if (e.type === CUSTOM_EVENT && typeof e.timestamp === 'number' && e.timestamp < MAX_SECONDS_TIMESTAMP) {
      return { ...event, timestamp: Math.round(e.timestamp * 1000) }
    }
    return event
  })
}

/** Une erreur à repérer sur la timeline du replay. `timestamp` en millisecondes (epoch). */
export interface ReplayMarker {
  timestamp: number
  label: string
  /** L'erreur affichée sur la page (les autres sont celles de la même session). */
  current?: boolean
}

/** Couleurs des repères sur la timeline de rrweb-player (clé = `data.tag` de l'événement personnalisé). */
const MARKER_TAGS = { error: '#dc2626', 'other-error': '#f59e0b' }
/** À l'ouverture, on se place un peu avant l'erreur pour voir ce qui l'a provoquée. */
const SEEK_LEAD_MS = 3000

/**
 * Ajoute un événement personnalisé (type 5) par erreur : rrweb-player l'affiche comme un repère coloré sur la
 * timeline. Un repère hors de l'enregistrement est ramené à son début ou à sa fin. `seekMs` est le décalage
 * (depuis le premier événement) où positionner le lecteur, ou `null` sans erreur courante.
 */
export function withMarkers(
  events: unknown[],
  markers: ReplayMarker[],
): { events: unknown[]; tags: Record<string, string>; seekMs: number | null } {
  if (markers.length === 0) return { events, tags: {}, seekMs: null }
  const stamps = events.map((e) => (e as { timestamp?: unknown })?.timestamp).filter((t): t is number => typeof t === 'number')
  const first = Math.min(...stamps)
  const last = Math.max(...stamps)
  const clamp = (t: number) => Math.min(Math.max(t, first), last)

  const added = markers.map((m) => ({
    type: CUSTOM_EVENT,
    timestamp: clamp(m.timestamp),
    data: { tag: m.current ? 'error' : 'other-error', payload: { label: m.label } },
  }))
  const sorted = [...events, ...added].sort(
    (a, b) => ((a as { timestamp?: number }).timestamp ?? 0) - ((b as { timestamp?: number }).timestamp ?? 0),
  )
  const current = added.find((_, i) => markers[i]!.current)
  const seekMs = current ? Math.max(0, current.timestamp - first - SEEK_LEAD_MS) : null
  return { events: sorted, tags: { ...MARKER_TAGS }, seekMs }
}
