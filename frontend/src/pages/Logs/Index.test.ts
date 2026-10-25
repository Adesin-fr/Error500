import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { resetInertiaMock, router } from '../../test/inertiaMock'
import Index from './Index.vue'

beforeEach(resetInertiaMock)

const props = {
  auth: { user: { id: 1, name: 'A', email: 'a@x.io', is_admin: false } },
  project: { id: 5, name: 'Shop', slug: 'shop' },
  can_manage: false,
  logs: [
    { id: 2, occurred_at: '2023-11-14T10:00:01Z', level: 'error', body: 'db connection lost', trace_id: '0af7651916cd43dd8448eb211c80319c', attributes: { port: { value: 5432, type: 'integer' } } },
    { id: 1, occurred_at: '2023-11-14T10:00:00Z', level: 'info', body: 'server started', trace_id: null, attributes: null },
  ],
  filters: { level: null, q: null },
}

describe('Logs/Index', () => {
  it('lists log lines with level, message and attributes', () => {
    const w = mount(Index, { props: props as never })
    const rows = w.findAll('.log-row')
    expect(rows).toHaveLength(2)
    expect(rows[0]!.text()).toContain('db connection lost')
    expect(rows[0]!.classes()).toContain('level-error')
    expect(rows[0]!.text()).toContain('port=5432')
    expect(rows[0]!.text()).toContain('0af76519')
  })

  it('filters by level and text through the server', async () => {
    const w = mount(Index, { props: props as never })
    await w.find('select[name="level"]').setValue('error')
    await w.find('input[name="q"]').setValue('db')
    await w.find('form').trigger('submit')
    expect(router.get).toHaveBeenCalledWith('/projects/5/logs', { level: 'error', q: 'db' }, expect.objectContaining({ preserveState: true }))
  })

  it('omits empty filters', async () => {
    const w = mount(Index, { props: props as never })
    await w.find('form').trigger('submit')
    expect(router.get).toHaveBeenCalledWith('/projects/5/logs', {}, expect.anything())
  })

  it('shows an empty state', () => {
    const w = mount(Index, { props: { ...props, logs: [] } as never })
    expect(w.text()).toContain('Aucun log')
  })
})
