import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { forms, resetInertiaMock } from '../../test/inertiaMock'
import Login from './Login.vue'

beforeEach(resetInertiaMock)

describe('Login', () => {
  it('submits the credentials to /login', async () => {
    const w = mount(Login, { props: { errors: {} } })
    await w.find('input[type="email"]').setValue('a@example.com')
    await w.find('input[type="password"]').setValue('correct horse')
    await w.find('form').trigger('submit')
    const form = forms[0]!
    expect(form.email).toBe('a@example.com')
    expect(form.password).toBe('correct horse')
    expect(form.post).toHaveBeenCalledWith('/login', expect.anything())
  })

  it('shows the error returned by the server', () => {
    const w = mount(Login, { props: { errors: { email: 'Adresse e-mail ou mot de passe incorrect.' } } })
    expect(w.text()).toContain('incorrect')
  })

  it('does not offer to register: accounts are created by an admin', () => {
    expect(mount(Login, { props: { errors: {} } }).text()).not.toMatch(/inscri/i)
  })
})
