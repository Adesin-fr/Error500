import { describe, expect, it } from 'vitest'
import { normalizeReplayEvents } from './replay'

describe('normalizeReplayEvents', () => {
  it('converts the seconds timestamps of Sentry custom events to milliseconds', () => {
    const events = [
      { type: 4, timestamp: 1792943180413 },
      { type: 5, timestamp: 1792943185.086, data: { tag: 'performanceSpan' } },
      { type: 3, timestamp: 1792943190365 },
    ]
    const out = normalizeReplayEvents(events) as Array<{ timestamp: number }>
    expect(out.map((e) => e.timestamp)).toEqual([1792943180413, 1792943185086, 1792943190365])
  })

  it('keeps custom events that are already in milliseconds', () => {
    const events = [{ type: 5, timestamp: 1792943185086 }]
    expect(normalizeReplayEvents(events)).toEqual(events)
  })

  it('only touches custom events', () => {
    const events = [{ type: 3, timestamp: 12 }, { type: 4, timestamp: 34 }]
    expect(normalizeReplayEvents(events)).toEqual(events)
  })

  it('does not mutate its input and tolerates malformed entries', () => {
    const events = [{ type: 5, timestamp: 1792943185.5 }, null, 'x', { type: 5 }] as unknown[]
    const out = normalizeReplayEvents(events)
    expect((events[0] as { timestamp: number }).timestamp).toBe(1792943185.5)
    expect(out[0]).toEqual({ type: 5, timestamp: 1792943185500 })
    expect(out.slice(1)).toEqual([null, 'x', { type: 5 }])
  })

  it('gives a session its real duration', () => {
    const events = [
      { type: 4, timestamp: 1792943180413 },
      { type: 5, timestamp: 1792943180.4138 },
      { type: 3, timestamp: 1792943198002 },
    ]
    const ts = normalizeReplayEvents(events).map((e) => (e as { timestamp: number }).timestamp)
    expect(Math.max(...ts) - Math.min(...ts)).toBeLessThan(20_000)
  })
})
