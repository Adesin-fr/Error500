import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { forms, resetInertiaMock, router } from '../../test/inertiaMock'
import Settings from './Settings.vue'

beforeEach(() => {
  resetInertiaMock()
  vi.stubGlobal('confirm', vi.fn(() => true))
})

const props = (over: Record<string, unknown> = {}) => ({
  auth: { user: { id: 1, name: 'A', email: 'a@x.io', is_admin: false } },
  project: { id: 5, name: 'Shop', slug: 'shop' },
  can_manage: true,
  dsn: 'https://pk@err.example.com/5',
  upload_url: 'https://err.example.com/api/5/sourcemaps/',
  upload_token: 'tok123',
  members: [{ user_id: 1, name: 'Alice', email: 'alice@example.com', role: 'owner' }, { user_id: 2, name: 'Bob', email: 'bob@example.com', role: 'member' }],
  sourcemaps: [{ id: 9, release: '1.0', name: '~/static/app.min.js.map', debug_id: null, size_bytes: 1536 }],
  notifications: true,
  errors: {},
  ...over,
})

describe('Projects/Settings', () => {
  it('shows the DSN to give to the SDK', () => {
    const w = mount(Settings, { props: props() as never })
    const inputs = w.findAll('input[readonly]').map((i) => (i.element as HTMLInputElement).value)
    expect(inputs).toContain('https://pk@err.example.com/5')
  })

  it('documents the source map upload with the project token', () => {
    const w = mount(Settings, { props: props() as never })
    expect(w.find('pre.snippet').text()).toContain('https://err.example.com/api/5/sourcemaps/')
    expect(w.text()).toContain('~/static/app.min.js.map')
    expect(w.text()).toContain('1.5 KB')
  })

  it('shows the email alert preference and turns it off', async () => {
    const w = mount(Settings, { props: props() as never })
    const box = w.find('input.notifications')
    expect((box.element as HTMLInputElement).checked).toBe(true)
    await box.setValue(false)
    expect(router.post).toHaveBeenCalledWith('/projects/5/notifications', { enabled: false }, expect.anything())
  })

  it('turns the alerts back on', async () => {
    const w = mount(Settings, { props: props({ notifications: false }) as never })
    await w.find('input.notifications').setValue(true)
    expect(router.post).toHaveBeenCalledWith('/projects/5/notifications', { enabled: true }, expect.anything())
  })

  it('hides the preference for someone who is not a member', () => {
    const w = mount(Settings, { props: props({ notifications: null }) as never })
    expect(w.find('input.notifications').exists()).toBe(false)
  })

  it('adds a member by email', async () => {
    const w = mount(Settings, { props: props() as never })
    await w.find('form.add-member input[name="email"]').setValue('carol@example.com')
    await w.find('form.add-member').trigger('submit')
    const form = forms.find((f) => 'role' in f)!
    expect(form.email).toBe('carol@example.com')
    expect(form.post).toHaveBeenCalledWith('/projects/5/members', expect.anything())
  })

  it('removes a member after confirmation', async () => {
    const w = mount(Settings, { props: props() as never })
    await w.find('button.remove-member-2').trigger('click')
    expect(router.delete).toHaveBeenCalledWith('/projects/5/members/2', expect.anything())
  })

  it('does not remove anything when the confirmation is declined', async () => {
    vi.stubGlobal('confirm', vi.fn(() => false))
    const w = mount(Settings, { props: props() as never })
    await w.find('button.remove-member-2').trigger('click')
    expect(router.delete).not.toHaveBeenCalled()
  })

  it('rotates keys and deletes source maps', async () => {
    const w = mount(Settings, { props: props() as never })
    await w.find('button.rotate-key').trigger('click')
    expect(router.post).toHaveBeenCalledWith('/projects/5/rotate-key', {}, expect.anything())
    await w.find('button.rotate-upload-token').trigger('click')
    expect(router.post).toHaveBeenCalledWith('/projects/5/rotate-upload-token', {}, expect.anything())
    await w.find('button.delete-sourcemap-9').trigger('click')
    expect(router.delete).toHaveBeenCalledWith('/projects/5/sourcemaps/9', expect.anything())
  })

  it('hides management for plain members', () => {
    const w = mount(Settings, { props: props({ can_manage: false, upload_token: null }) as never })
    expect(w.find('form.add-member').exists()).toBe(false)
    expect(w.find('button.rotate-key').exists()).toBe(false)
    expect(w.find('pre.snippet').exists()).toBe(false)
    expect(w.findAll('input[readonly]').length).toBeGreaterThan(0) // le DSN reste visible
  })
})
