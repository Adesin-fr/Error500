import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { resetInertiaMock } from '../../test/inertiaMock'
import Index from './Index.vue'

beforeEach(resetInertiaMock)

const common = { auth: { user: { id: 1, name: 'A', email: 'a@x.io', is_admin: false } }, project: { id: 5, name: 'Shop', slug: 'shop' }, can_manage: false }

describe('Replays/Index', () => {
  it('lists replays with user, duration and first page', () => {
    const rid = 'ab'.repeat(16)
    const w = mount(Index, {
      props: {
        ...common,
        replays: [{ replay_id: rid, first_seen: '2023-11-14T10:00:00Z', last_seen: '2023-11-14T10:02:05Z', duration_seconds: 125, segment_count: 3,
          urls: ['https://app.example.com/checkout', 'https://app.example.com/done'], live: false, user_label: 'jane@example.com', browser: 'Chrome 120', os: 'macOS 14', release_name: '1.0', environment: 'prod', error_ids: [] }],
      } as never,
    })
    expect(w.find(`a[href="/projects/5/replays/${rid}"]`).exists()).toBe(true)
    expect(w.text()).toContain('jane@example.com')
    expect(w.text()).toContain('2m 05s')
    expect(w.text()).toContain('app.example.com/checkout')
    expect(w.text()).toContain('Chrome 120')
  })

  it('flags the sessions that are in progress', () => {
    const base = { first_seen: '2023-11-14T10:00:00Z', last_seen: '2023-11-14T10:02:05Z', duration_seconds: 5, segment_count: 1, urls: [], user_label: null,
      browser: null, os: null, release_name: null, environment: null, error_ids: [] }
    const w = mount(Index, {
      props: { ...common, replays: [{ ...base, replay_id: 'a'.repeat(32), live: true }, { ...base, replay_id: 'b'.repeat(32), live: false }] } as never,
    })
    const rows = w.findAll('tbody tr')
    expect(rows[0]!.find('.live-badge').exists()).toBe(true)
    expect(rows[0]!.text()).toContain('En direct')
    expect(rows[1]!.find('.live-badge').exists()).toBe(false)
  })

  it('explains how replays get here when there are none', () => {
    const w = mount(Index, { props: { ...common, replays: [] } as never })
    expect(w.text()).toContain('Aucun replay')
  })
})
