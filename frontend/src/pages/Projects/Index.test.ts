import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { forms, resetInertiaMock } from '../../test/inertiaMock'
import Index from './Index.vue'

beforeEach(resetInertiaMock)

const projects = [
  { id: 1, name: 'Shop', slug: 'shop', unresolved_count: 3 },
  { id: 2, name: 'Blog', slug: 'blog', unresolved_count: 0 },
]

describe('Projects/Index', () => {
  it('lists projects linking to their issues, with unresolved counts', () => {
    const w = mount(Index, { props: { projects, errors: {} } })
    const link = w.find('a[href="/projects/1/issues"]')
    expect(link.text()).toContain('Shop')
    expect(w.text()).toContain('3 non résolues')
    expect(w.text()).toContain('Aucune erreur non résolue')
  })

  it('invites to create a first project when there is none', () => {
    const w = mount(Index, { props: { projects: [], errors: {} } })
    expect(w.text()).toContain('Aucun projet')
  })

  it('creates a project', async () => {
    const w = mount(Index, { props: { projects: [], errors: {} } })
    await w.find('input[name="name"]').setValue('New one')
    await w.find('form').trigger('submit')
    expect(forms[0]!.name).toBe('New one')
    expect(forms[0]!.post).toHaveBeenCalledWith('/projects', expect.anything())
  })
})
