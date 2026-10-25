import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { forms, resetInertiaMock, router } from '../../test/inertiaMock'
import Show from './Show.vue'

beforeEach(resetInertiaMock)

const off = { available: true, enabled: false, recovery_remaining: 0 }

describe('Account/Show', () => {
  it('explains that SECRET_KEY is missing and hides the enable button', () => {
    const w = mount(Show, { props: { two_factor: { ...off, available: false }, recovery_codes: null, errors: {} } })
    expect(w.find('button.enable-2fa').exists()).toBe(false)
    expect(w.text()).toContain('SECRET_KEY')
  })

  it('shows the server error when setup is refused', () => {
    const w = mount(Show, { props: { two_factor: off, recovery_codes: null, errors: { setup: 'SECRET_KEY manquante' } } })
    expect(w.text()).toContain('SECRET_KEY manquante')
  })

  it('offers to enable 2FA when it is off', async () => {
    const w = mount(Show, { props: { two_factor: off, recovery_codes: null, errors: {} } })
    await w.find('button.enable-2fa').trigger('click')
    expect(router.post).toHaveBeenCalledWith('/account/2fa/setup', {}, expect.anything())
  })

  it('shows the QR code and secret while setup is pending, and confirms with a code', async () => {
    const w = mount(Show, {
      props: {
        two_factor: { ...off, secret: 'JBSWY3DPEHPK3PXP', otpauth_uri: 'otpauth://totp/x', qr_svg: '<svg class="qr"></svg>' },
        recovery_codes: null,
        errors: {},
      },
    })
    expect(w.find('.qr').exists()).toBe(true)
    expect(w.text()).toContain('JBSWY3DPEHPK3PXP')
    await w.find('input[name="code"]').setValue('123456')
    await w.find('form.confirm-2fa').trigger('submit')
    expect(forms[0]!.code).toBe('123456')
    expect(forms[0]!.post).toHaveBeenCalledWith('/account/2fa/enable', expect.anything())
  })

  it('shows the recovery codes once they are issued', () => {
    const w = mount(Show, {
      props: { two_factor: { available: true, enabled: true, recovery_remaining: 2 }, recovery_codes: ['aaaaa-bbbbb', 'ccccc-ddddd'], errors: {} },
    })
    expect(w.text()).toContain('aaaaa-bbbbb')
    expect(w.text()).toContain('ccccc-ddddd')
  })

  it('requires the password to disable 2FA', async () => {
    const w = mount(Show, { props: { two_factor: { available: true, enabled: true, recovery_remaining: 8 }, recovery_codes: null, errors: {} } })
    expect(w.text()).toContain('8')
    await w.find('input[name="password"]').setValue('correct horse')
    await w.find('form.disable-2fa').trigger('submit')
    expect(forms[1]!.password).toBe('correct horse')
    expect(forms[1]!.post).toHaveBeenCalledWith('/account/2fa/disable', expect.anything())
  })
})
