import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { forms, resetInertiaMock } from '../../test/inertiaMock'
import TwoFactor from './TwoFactor.vue'

beforeEach(resetInertiaMock)

describe('TwoFactor (login)', () => {
  it('submits the challenge and the code to /login/two-factor', async () => {
    const w = mount(TwoFactor, { props: { challenge: 'abc123', errors: {} } })
    await w.find('input[name="code"]').setValue('123 456')
    await w.find('form').trigger('submit')
    const form = forms[0]!
    expect(form.challenge).toBe('abc123')
    expect(form.code).toBe('123 456')
    expect(form.post).toHaveBeenCalledWith('/login/two-factor', expect.anything())
  })

  it('asks for an authenticator or recovery code and shows the server error', () => {
    const w = mount(TwoFactor, { props: { challenge: 'abc', errors: { code: 'Code incorrect.' } } })
    expect(w.text()).toMatch(/code de secours/i)
    expect(w.text()).toContain('Code incorrect.')
  })
})
