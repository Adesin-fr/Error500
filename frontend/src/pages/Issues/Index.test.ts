import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { resetInertiaMock } from '../../test/inertiaMock'
import Index from './Index.vue'

beforeEach(resetInertiaMock)

const base = {
  auth: { user: { id: 1, name: 'A', email: 'a@x.io', is_admin: false } },
  project: { id: 5, name: 'Shop', slug: 'shop' },
  can_manage: true,
  issues: [
    { id: 11, title: 'TypeError: x is undefined', level: 'error', status: 'unresolved', first_seen: '2023-11-14T10:00:00Z', last_seen: '2023-11-14T11:00:00Z', event_count: 42 },
  ],
  status: 'unresolved',
  page: 1,
  pages: 3,
  total: 60,
  counts: { unresolved: 60, resolved: 4, ignored: 1 },
}

describe('Issues/Index', () => {
  it('lists issues linking to their detail page', () => {
    const w = mount(Index, { props: base as never })
    const link = w.find('a[href="/projects/5/issues/11"]')
    expect(link.text()).toContain('TypeError: x is undefined')
    expect(w.text()).toContain('42')
  })

  it('offers the status tabs with counts and marks the current one', () => {
    const w = mount(Index, { props: base as never })
    const tabs = w.findAll('.status-tab')
    expect(tabs.map((t) => t.attributes('href'))).toEqual([
      '/projects/5/issues?status=unresolved',
      '/projects/5/issues?status=resolved',
      '/projects/5/issues?status=ignored',
    ])
    expect(tabs[0]!.classes()).toContain('active')
    expect(tabs[1]!.text()).toContain('4')
  })

  it('paginates while keeping the status filter', () => {
    const w = mount(Index, { props: base as never })
    expect(w.find('a[rel="next"]').attributes('href')).toBe('/projects/5/issues?status=unresolved&page=2')
  })

  it('shows an empty state with the DSN hint', () => {
    const w = mount(Index, { props: { ...base, issues: [], total: 0, pages: 1, counts: { unresolved: 0, resolved: 0, ignored: 0 } } as never })
    expect(w.text()).toContain('Aucune issue')
  })
})
