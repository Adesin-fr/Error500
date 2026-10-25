import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { forms, resetInertiaMock } from '../../test/inertiaMock'
import Users from './Users.vue'

beforeEach(resetInertiaMock)

const props = {
  auth: { user: { id: 1, name: 'Root', email: 'root@x.io', is_admin: true } },
  users: [
    { id: 1, name: 'Root', email: 'root@x.io', is_admin: true, created_at: '2023-11-14T10:00:00Z' },
    { id: 2, name: 'Eve', email: 'eve@x.io', is_admin: false, created_at: '2023-11-15T10:00:00Z' },
  ],
  errors: {},
}

describe('Admin/Users', () => {
  it('lists users and flags administrators', () => {
    const w = mount(Users, { props: props as never })
    const rows = w.findAll('tbody tr')
    expect(rows).toHaveLength(2)
    expect(rows[0]!.text()).toContain('Admin')
    expect(rows[1]!.text()).toContain('eve@x.io')
  })

  it('creates a user', async () => {
    const w = mount(Users, { props: props as never })
    await w.find('input[name="name"]').setValue('Zoe')
    await w.find('input[type="email"]').setValue('zoe@x.io')
    await w.find('input[type="password"]').setValue('correct horse')
    await w.find('input[type="checkbox"]').setValue(true)
    await w.find('form').trigger('submit')
    expect(forms[0]!.is_admin).toBe(true)
    expect(forms[0]!.post).toHaveBeenCalledWith('/admin/users', expect.anything())
  })
})
