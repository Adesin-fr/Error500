import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../test/inertiaMock')).mock)
import ProjectNav from './ProjectNav.vue'

describe('ProjectNav', () => {
  const project = { id: 7, name: 'Shop', slug: 'shop' }

  it('links to the sections of the project', () => {
    const w = mount(ProjectNav, { props: { project, active: 'issues' } })
    const hrefs = w.findAll('a').map((a) => a.attributes('href'))
    expect(hrefs).toEqual([
      '/projects/7/issues',
      '/projects/7/replays',
      '/projects/7/logs',
      '/projects/7/settings',
    ])
  })

  it('marks the active section', () => {
    const w = mount(ProjectNav, { props: { project, active: 'replays' } })
    const active = w.findAll('a.active')
    expect(active).toHaveLength(1)
    expect(active[0]!.text()).toBe('Replays')
  })

  it('shows the project name', () => {
    expect(mount(ProjectNav, { props: { project, active: 'logs' } }).text()).toContain('Shop')
  })
})
