import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { resetInertiaMock } from '../../test/inertiaMock'
import Show from './Show.vue'

beforeEach(resetInertiaMock)

const props = {
  auth: { user: { id: 1, name: 'A', email: 'a@x.io', is_admin: false } },
  project: { id: 5, name: 'Shop', slug: 'shop' },
  can_manage: false,
  replay: { replay_id: 'ab'.repeat(16), first_seen: '2023-11-14T10:00:00Z', last_seen: '2023-11-14T10:02:05Z', duration_seconds: 125, segment_count: 3,
    urls: ['https://app.example.com/a'], user_label: 'jane@example.com', browser: 'Chrome 120', os: 'macOS 14', release_name: '1.0', environment: 'prod', error_ids: [] },
  events_url: '/projects/5/replays/abc/events',
  errors: [{ event_id: 'e'.repeat(32), issue_id: 11, title: 'TypeError: boom' }],
}

describe('Replays/Show', () => {
  it('embeds the player on the events URL', () => {
    const w = mount(Show, { props: props as never, global: { stubs: { ReplayPlayer: { props: ['eventsUrl'], template: '<div class="player-stub">{{ eventsUrl }}</div>' } } } })
    expect(w.find('.player-stub').text()).toBe('/projects/5/replays/abc/events')
  })

  it('shows who/where and links the errors captured during the session', () => {
    const w = mount(Show, { props: props as never, global: { stubs: { ReplayPlayer: true } } })
    expect(w.text()).toContain('jane@example.com')
    expect(w.text()).toContain('macOS 14')
    const link = w.find(`a[href="/projects/5/issues/11?event=${'e'.repeat(32)}"]`)
    expect(link.text()).toContain('TypeError: boom')
  })
})
