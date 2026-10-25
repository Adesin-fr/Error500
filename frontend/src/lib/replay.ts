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
