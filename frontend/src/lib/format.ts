const pad = (n: number) => String(n).padStart(2, '0')

export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.round(totalSeconds))
  if (s < 60) return `${s}s`
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  if (h > 0) return `${h}h ${pad(m)}m`
  return `${m}m ${pad(s % 60)}s`
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB']
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${value.toFixed(1)} ${units[unit]}`
}

function parts(iso: string, timeZone?: string): Record<string, string> {
  const fmt = new Intl.DateTimeFormat('fr-FR', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hourCycle: 'h23',
  })
  return Object.fromEntries(fmt.formatToParts(new Date(iso)).map((p) => [p.type, p.value]))
}

export function formatDateTime(iso: string, timeZone?: string): string {
  const p = parts(iso, timeZone)
  return `${p.day}/${p.month}/${p.year} ${p.hour}:${p.minute}:${p.second}`
}

export function relativeTime(iso: string, now: Date = new Date()): string {
  const seconds = (now.getTime() - new Date(iso).getTime()) / 1000
  if (seconds < 10) return "à l'instant"
  if (seconds < 3600) return `il y a ${Math.max(1, Math.round(seconds / 60))} min`
  if (seconds < 86400) return `il y a ${Math.round(seconds / 3600)} h`
  if (seconds < 30 * 86400) return `il y a ${Math.round(seconds / 86400)} j`
  const p = parts(iso)
  return `${p.day}/${p.month}/${p.year}`
}

export function shortId(id: string): string {
  return id.slice(0, 8)
}

export type LevelClass = 'fatal' | 'error' | 'warning' | 'info' | 'debug'

export function levelClass(level: string): LevelClass {
  switch (level.toLowerCase()) {
    case 'fatal':
      return 'fatal'
    case 'error':
      return 'error'
    case 'warning':
    case 'warn':
      return 'warning'
    case 'debug':
    case 'trace':
      return 'debug'
    default:
      return 'info'
  }
}
