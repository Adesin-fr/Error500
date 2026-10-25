import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const players: Array<{ options: any; $destroy: ReturnType<typeof vi.fn> }> = []

vi.mock('@sentry-internal/rrweb-player', () => ({
  default: class {
    options: any
    $destroy = vi.fn()
    constructor(options: any) {
      this.options = options
      players.push(this)
    }
  },
}))
vi.mock('@sentry-internal/rrweb-player/dist/style.css', () => ({}))

import ReplayPlayer from './ReplayPlayer.vue'

const respond = (body: unknown, ok = true) =>
  vi.fn().mockResolvedValue({ ok, status: ok ? 200 : 404, json: async () => body })

beforeEach(() => {
  players.length = 0
})
afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ReplayPlayer', () => {
  it('loads the events and starts the rrweb player', async () => {
    const events = [{ type: 4, timestamp: 1 }, { type: 2, timestamp: 2 }]
    const fetchMock = respond(events)
    vi.stubGlobal('fetch', fetchMock)
    const w = mount(ReplayPlayer, { props: { eventsUrl: '/projects/1/replays/abc/events' } })
    expect(w.text()).toContain('Chargement')
    await flushPromises()
    expect(fetchMock).toHaveBeenCalledWith('/projects/1/replays/abc/events', expect.objectContaining({ credentials: 'same-origin' }))
    expect(players).toHaveLength(1)
    expect(players[0]!.options.props.events).toEqual(events)
    expect(players[0]!.options.target).toBeInstanceOf(HTMLElement)
    expect(w.text()).not.toContain('Chargement')
  })

  it('hands rrweb custom events with timestamps in milliseconds, so the duration is the real one', async () => {
    vi.stubGlobal('fetch', respond([
      { type: 4, timestamp: 1792943180413 },
      { type: 5, timestamp: 1792943185.086, data: { tag: 'breadcrumb' } },
      { type: 2, timestamp: 1792943190365 },
    ]))
    mount(ReplayPlayer, { props: { eventsUrl: '/x' } })
    await flushPromises()
    const ts = (players[0]!.options.props.events as Array<{ timestamp: number }>).map((e) => e.timestamp)
    expect(ts).toEqual([1792943180413, 1792943185086, 1792943190365])
  })

  it('sizes the player from the visible width of its card, not from the hidden host', async () => {
    // Comme dans un vrai navigateur : un élément masqué (display: none) mesure 0 px.
    const widthOf = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) {
      return this.style.display === 'none' ? 0 : 640
    })
    vi.stubGlobal('fetch', respond([{ type: 4, timestamp: 1 }, { type: 2, timestamp: 2 }]))
    mount(ReplayPlayer, { props: { eventsUrl: '/x' } })
    await flushPromises()
    widthOf.mockRestore()
    expect(players[0]!.options.props.width).toBe(640)
    expect(players[0]!.options.props.height).toBe(384)
  })

  it('does not start the player when the replay is too short to play', async () => {
    vi.stubGlobal('fetch', respond([{ type: 4, timestamp: 1 }]))
    const w = mount(ReplayPlayer, { props: { eventsUrl: '/x' } })
    await flushPromises()
    expect(players).toHaveLength(0)
    expect(w.text()).toContain('trop court')
  })

  it('reports a loading failure', async () => {
    vi.stubGlobal('fetch', respond(null, false))
    const w = mount(ReplayPlayer, { props: { eventsUrl: '/x' } })
    await flushPromises()
    expect(players).toHaveLength(0)
    expect(w.text()).toContain('Impossible de charger')
  })

  it('destroys the player on unmount', async () => {
    vi.stubGlobal('fetch', respond([{ type: 4, timestamp: 1 }, { type: 2, timestamp: 2 }]))
    const w = mount(ReplayPlayer, { props: { eventsUrl: '/x' } })
    await flushPromises()
    w.unmount()
    expect(players[0]!.$destroy).toHaveBeenCalled()
  })
})
