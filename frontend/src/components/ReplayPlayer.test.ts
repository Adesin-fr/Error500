import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const players: Array<{
  options: any
  $destroy: ReturnType<typeof vi.fn>
  addEvent: ReturnType<typeof vi.fn>
  startLive: ReturnType<typeof vi.fn>
  setSpeed: ReturnType<typeof vi.fn>
  goto: ReturnType<typeof vi.fn>
}> = []

/** EventSource factice : les tests y « poussent » des messages comme le ferait le serveur. */
class FakeEventSource {
  static instances: FakeEventSource[] = []
  url: string
  close = vi.fn()
  onerror: (() => void) | null = null
  private listeners: Record<string, Array<(e: { data: string }) => void>> = {}
  constructor(url: string) {
    this.url = url
    FakeEventSource.instances.push(this)
  }
  addEventListener(type: string, fn: (e: { data: string }) => void) {
    ;(this.listeners[type] ??= []).push(fn)
  }
  emit(type: string, data: unknown) {
    for (const fn of this.listeners[type] ?? []) fn({ data: JSON.stringify(data) })
  }
}

vi.mock('@sentry-internal/rrweb-player', () => ({
  default: class {
    options: any
    $destroy = vi.fn()
    addEvent = vi.fn()
    startLive = vi.fn()
    setSpeed = vi.fn()
    goto = vi.fn()
    constructor(options: any) {
      this.options = options
      players.push(this as never)
    }
    getReplayer() {
      return { startLive: this.startLive }
    }
  },
}))
vi.mock('@sentry-internal/rrweb-player/dist/style.css', () => ({}))

import ReplayPlayer from './ReplayPlayer.vue'

const respond = (body: unknown, ok = true) =>
  vi.fn().mockResolvedValue({ ok, status: ok ? 200 : 404, json: async () => body })

beforeEach(() => {
  players.length = 0
  FakeEventSource.instances.length = 0
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

  describe('error markers', () => {
    // Timestamps réalistes (ms epoch) : sous 1e11 le lecteur les prendrait pour des secondes.
    const T0 = 1_792_000_000_000
    const recording = [{ type: 4, timestamp: T0 }, { type: 2, timestamp: T0 + 100 }, { type: 3, timestamp: T0 + 30_000 }]

    it('tags the timeline at the instant of the error and starts a few seconds before it', async () => {
      vi.stubGlobal('fetch', respond(recording))
      mount(ReplayPlayer, { props: { eventsUrl: '/x', markers: [{ timestamp: T0 + 20_000, label: 'TypeError: boom', current: true }] } })
      await flushPromises()
      const p = players[0]!
      const custom = (p.options.props.events as Array<{ type: number; timestamp: number; data?: { tag: string } }>).filter((e) => e.type === 5)
      expect(custom).toHaveLength(1)
      expect(custom[0]!.timestamp).toBe(T0 + 20_000)
      expect(custom[0]!.data!.tag).toBe('error')
      expect(Object.keys(p.options.props.tags)).toContain('error')
      expect(p.goto).toHaveBeenCalledWith(17_000)
    })

    it('does not touch the player when there are no markers', async () => {
      vi.stubGlobal('fetch', respond(recording))
      mount(ReplayPlayer, { props: { eventsUrl: '/x' } })
      await flushPromises()
      expect(players[0]!.options.props.tags).toBeUndefined()
      expect(players[0]!.goto).not.toHaveBeenCalled()
    })
  })

  describe('live mode', () => {
    const SEG0 = [{ type: 4, timestamp: 1792943180413 }, { type: 2, timestamp: 1792943180414 }]
    const SEG1 = [{ type: 3, timestamp: 1792943185.5 }, { type: 3, timestamp: 1792943186000 }]

    const live = () => {
      vi.stubGlobal('EventSource', FakeEventSource)
      const fetchMock = vi.fn()
      vi.stubGlobal('fetch', fetchMock)
      const w = mount(ReplayPlayer, { props: { eventsUrl: '/events', streamUrl: '/stream', live: true } })
      return { w, fetchMock, es: FakeEventSource.instances[0]! }
    }

    it('follows the stream instead of loading the recorded events', async () => {
      const { w, fetchMock, es } = live()
      await flushPromises()
      expect(fetchMock).not.toHaveBeenCalled()
      expect(FakeEventSource.instances).toHaveLength(1)
      expect(es.url).toBe('/stream')
      expect(w.text()).toContain('En attente des premières données')
      expect(players).toHaveLength(0)
    })

    it('starts the player in live mode on the first segment and feeds it every event', async () => {
      const { es } = live()
      es.emit('status', { live: true })
      es.emit('segment', { segment_id: 0, events: SEG0 })
      await flushPromises()
      expect(players).toHaveLength(1)
      expect(players[0]!.options.props.liveMode).toBe(true)
      expect(players[0]!.startLive).toHaveBeenCalledTimes(1)
      // baseline = plus ancien timestamp : rrweb applique « en rattrapage » (sans mutations du DOM) tout ce qui la précède
      expect(players[0]!.startLive).toHaveBeenCalledWith(1792943180413)
      expect(players[0]!.addEvent.mock.calls.map((c) => c[0])).toEqual(SEG0)
    })

    it('plays a short first segment at normal speed', async () => {
      const { es } = live()
      es.emit('segment', { segment_id: 0, events: SEG0 })
      await flushPromises()
      expect(players[0]!.options.props.speed).toBeUndefined()
      vi.useFakeTimers()
      await vi.advanceTimersByTimeAsync(10_000)
      vi.useRealTimers()
      expect(players[0]!.setSpeed).not.toHaveBeenCalled()
    })

    it('catches up on a long history at 8x, then returns to normal speed', async () => {
      vi.useFakeTimers()
      const { es } = live()
      const history = [{ type: 4, timestamp: 1_000_000 }, { type: 2, timestamp: 1_000_001 }, { type: 3, timestamp: 1_080_000 }]
      es.emit('segment', { segment_id: 0, events: history })
      await vi.advanceTimersByTimeAsync(0)
      expect(players[0]!.options.props.speed).toBe(8)
      // 80 s d'historique à ×8 : ~10 s de rattrapage
      await vi.advanceTimersByTimeAsync(9_000)
      expect(players[0]!.setSpeed).not.toHaveBeenCalled()
      await vi.advanceTimersByTimeAsync(2_000)
      expect(players[0]!.setSpeed).toHaveBeenCalledWith(1)
      vi.useRealTimers()
    })

    it('appends the following segments to the same player, with millisecond timestamps', async () => {
      const { es } = live()
      es.emit('segment', { segment_id: 0, events: SEG0 })
      await flushPromises()
      es.emit('segment', { segment_id: 1, events: [{ type: 5, timestamp: 1792943185.5 }, SEG1[1]] })
      await flushPromises()
      expect(players).toHaveLength(1)
      const added = players[0]!.addEvent.mock.calls.map((c) => c[0].timestamp)
      expect(added).toEqual([1792943180413, 1792943180414, 1792943185500, 1792943186000])
    })

    it('ignores a segment it has already received (reconnection)', async () => {
      const { es } = live()
      es.emit('segment', { segment_id: 0, events: SEG0 })
      await flushPromises()
      es.emit('segment', { segment_id: 0, events: SEG0 })
      await flushPromises()
      expect(players[0]!.addEvent).toHaveBeenCalledTimes(2)
    })

    it('handles segments that arrive while the player is still loading', async () => {
      const { es } = live()
      es.emit('segment', { segment_id: 0, events: SEG0 })
      es.emit('segment', { segment_id: 1, events: SEG1 })
      await flushPromises()
      expect(players).toHaveLength(1)
      expect(players[0]!.addEvent).toHaveBeenCalledTimes(4)
    })

    it('shows whether the session is live or quiet', async () => {
      const { w, es } = live()
      es.emit('status', { live: true })
      await flushPromises()
      expect(w.find('.live-status').text()).toContain('En direct')
      es.emit('status', { live: false })
      await flushPromises()
      expect(w.find('.live-status').text()).toContain('inactive')
    })

    it('says so when the connection drops, and recovers on the next message', async () => {
      const { w, es } = live()
      es.onerror?.()
      await flushPromises()
      expect(w.text()).toContain('Connexion interrompue')
      es.emit('status', { live: true })
      await flushPromises()
      expect(w.text()).not.toContain('Connexion interrompue')
    })

    it('closes the stream and destroys the player when leaving the page', async () => {
      const { w, es } = live()
      es.emit('segment', { segment_id: 0, events: SEG0 })
      await flushPromises()
      w.unmount()
      expect(es.close).toHaveBeenCalled()
      expect(players[0]!.$destroy).toHaveBeenCalled()
    })

    it('does not touch the stream when the replay is not live', async () => {
      vi.stubGlobal('EventSource', FakeEventSource)
      vi.stubGlobal('fetch', respond([{ type: 4, timestamp: 1 }, { type: 2, timestamp: 2 }]))
      mount(ReplayPlayer, { props: { eventsUrl: '/events', streamUrl: '/stream', live: false } })
      await flushPromises()
      expect(FakeEventSource.instances).toHaveLength(0)
      expect(players[0]!.options.props.liveMode).toBeUndefined()
    })
  })
})
