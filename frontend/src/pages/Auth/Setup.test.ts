import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { forms, resetInertiaMock } from '../../test/inertiaMock'
import Setup from './Setup.vue'

beforeEach(resetInertiaMock)

describe('Setup', () => {
  it('creates the first account through /setup', async () => {
    const w = mount(Setup, { props: { errors: {} } })
    await w.find('input[name="name"]').setValue('Root')
    await w.find('input[type="email"]').setValue('root@example.com')
    await w.find('input[type="password"]').setValue('correct horse')
    await w.find('form').trigger('submit')
    expect(forms[0]!.name).toBe('Root')
    expect(forms[0]!.post).toHaveBeenCalledWith('/setup', expect.anything())
  })

  it('shows validation errors per field', () => {
    const w = mount(Setup, { props: { errors: { password: '8 caractères minimum.' } } })
    expect(w.text()).toContain('8 caractères minimum.')
  })
})
