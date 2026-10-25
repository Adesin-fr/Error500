import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { resetInertiaMock, router } from '../../test/inertiaMock'
import Index from './Index.vue'

beforeEach(resetInertiaMock)

const noFilter = { filters: { user: null }, users: [] }
const common = { ...noFilter, auth: { user: { id: 1, name: 'A', email: 'a@x.io', is_admin: false } }, project: { id: 5, name: 'Shop', slug: 'shop' }, can_manage: false }

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

  describe('user filter', () => {
    const base = { first_seen: '2023-11-14T10:00:00Z', last_seen: '2023-11-14T10:02:05Z', duration_seconds: 5, segment_count: 1, urls: [], user_label: 'jane@example.com',
      browser: null, os: null, release_name: null, environment: null, error_ids: [], live: false }
    const users = [{ label: 'jane@example.com', count: 3 }, { label: 'john@example.com', count: 1 }]
    const withReplays = (over: Record<string, unknown> = {}) =>
      mount(Index, { props: { ...common, replays: [{ ...base, replay_id: 'a'.repeat(32) }], users, ...over } as never })

    it('puts a user filter at the top of the page, suggesting the known users', () => {
      const w = withReplays()
      const input = w.find('form.user-filter input[name="user"]')
      expect(input.exists()).toBe(true)
      const options = w.findAll('datalist option').map((o) => o.attributes('value'))
      expect(options).toEqual(['jane@example.com', 'john@example.com'])
      expect(w.find('datalist').text() + w.html()).toContain('3 sessions')
    })

    it('shows the active filter in the field', () => {
      const w = withReplays({ filters: { user: 'jane' } })
      expect((w.find('input[name="user"]').element as HTMLInputElement).value).toBe('jane')
    })

    it('applies the filter through a visit that keeps the page state', async () => {
      const w = withReplays()
      await w.find('input[name="user"]').setValue('  john ')
      await w.find('form.user-filter').trigger('submit')
      expect(router.get).toHaveBeenCalledWith('/projects/5/replays', { user: 'john' }, expect.objectContaining({ preserveState: true, replace: true }))
    })

    it('submitting an empty field clears the filter', async () => {
      const w = withReplays({ filters: { user: 'jane' } })
      await w.find('input[name="user"]').setValue('')
      await w.find('form.user-filter').trigger('submit')
      expect(router.get).toHaveBeenCalledWith('/projects/5/replays', {}, expect.anything())
    })

    it('resets the filter with its button, shown only while a filter is active', async () => {
      expect(withReplays().find('button.reset-filter').exists()).toBe(false)
      const w = withReplays({ filters: { user: 'jane' } })
      await w.find('button.reset-filter').trigger('click')
      expect(router.get).toHaveBeenCalledWith('/projects/5/replays', {}, expect.anything())
    })

    it('says no session matches instead of explaining how to enable replays', () => {
      const w = mount(Index, { props: { ...common, replays: [], filters: { user: 'zoe' }, users } as never })
      expect(w.text()).toContain('Aucun replay pour « zoe »')
      expect(w.text()).not.toContain('replayIntegration')
      expect(w.find('form.user-filter').exists()).toBe(true)
    })

    it('hides the filter when the project has no replay at all', () => {
      const w = mount(Index, { props: { ...common, replays: [] } as never })
      expect(w.find('form.user-filter').exists()).toBe(false)
    })
  })
})
