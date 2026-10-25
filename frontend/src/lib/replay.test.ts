import { describe, expect, it } from 'vitest'
import { normalizeReplayEvents, withMarkers } from './replay'

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

describe('withMarkers', () => {
  const events = [
    { type: 4, timestamp: 1_000_000 },
    { type: 2, timestamp: 1_000_100 },
    { type: 3, timestamp: 1_030_000 },
  ]

  it('adds a timeline tag at the instant of each error, keeping the events sorted', () => {
    const out = withMarkers(events, [
      { timestamp: 1_020_000, label: 'TypeError', current: true },
      { timestamp: 1_010_000, label: 'RangeError' },
    ])
    const stamps = (out.events as Array<{ timestamp: number }>).map((e) => e.timestamp)
    expect(stamps).toEqual([1_000_000, 1_000_100, 1_010_000, 1_020_000, 1_030_000])
    const added = (out.events as Array<{ type: number; data?: { tag: string; payload: { label: string } } }>).filter((e) => e.type === 5)
    expect(added.map((e) => [e.data!.tag, e.data!.payload.label])).toEqual([['other-error', 'RangeError'], ['error', 'TypeError']])
  })

  it('gives each tag a colour for the timeline', () => {
    const out = withMarkers(events, [{ timestamp: 1_020_000, label: 'x', current: true }])
    expect(out.tags).toEqual({ error: expect.stringMatching(/^#/), 'other-error': expect.stringMatching(/^#/) })
    expect(out.tags.error).not.toBe(out['tags']['other-error'])
  })

  it('seeks a few seconds before the current error', () => {
    const out = withMarkers(events, [{ timestamp: 1_020_000, label: 'x', current: true }])
    expect(out.seekMs).toBe(17_000)
  })

  it('does not seek past the start, and does not seek without a current error', () => {
    expect(withMarkers(events, [{ timestamp: 1_001_000, label: 'x', current: true }]).seekMs).toBe(0)
    expect(withMarkers(events, [{ timestamp: 1_020_000, label: 'x' }]).seekMs).toBeNull()
  })

  it('keeps a marker that falls outside the recording on the timeline', () => {
    const out = withMarkers(events, [{ timestamp: 5_000_000, label: 'late', current: true }, { timestamp: 1, label: 'early' }])
    const stamps = (out.events as Array<{ timestamp: number }>).map((e) => e.timestamp)
    expect(Math.min(...stamps)).toBe(1_000_000)
    expect(Math.max(...stamps)).toBe(1_030_000)
    expect(stamps).toHaveLength(5)
  })

  it('leaves the events untouched without markers, and does not mutate its input', () => {
    expect(withMarkers(events, []).events).toEqual(events)
    withMarkers(events, [{ timestamp: 1_020_000, label: 'x' }])
    expect(events).toHaveLength(3)
  })
})
