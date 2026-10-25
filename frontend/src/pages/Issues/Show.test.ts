import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@inertiajs/vue3', async () => (await import('../../test/inertiaMock')).mock)
import { resetInertiaMock, router } from '../../test/inertiaMock'
import Show from './Show.vue'

beforeEach(resetInertiaMock)

const props = {
  auth: { user: { id: 1, name: 'A', email: 'a@x.io', is_admin: false } },
  project: { id: 5, name: 'Shop', slug: 'shop' },
  can_manage: true,
  issue: { id: 11, title: 'TypeError: x is undefined', level: 'error', status: 'unresolved', first_seen: '2023-11-14T10:00:00Z', last_seen: '2023-11-14T11:00:00Z', event_count: 2 },
  event: {
    event_id: 'a'.repeat(32), occurred_at: '2023-11-14T11:00:00Z', release: '1.0', environment: 'prod', platform: 'javascript', message: null,
    user: { email: 'jane@example.com' }, request: { url: 'https://app.example.com/checkout', method: 'GET' },
    tags: [{ key: 'browser', value: 'Chrome' }], breadcrumbs: [{ category: 'ui.click', message: 'button#pay' }],
    contexts: { browser: { name: 'Chrome', version: '120' }, os: {}, device: {}, runtime: {} },
  },
  exceptions: [{ type: 'TypeError', value: 'x is undefined', frames: [{ filename: 'src/app.ts', function: 'pay', lineno: 3, colno: 5, in_app: true, pre_context: [], context_line: 'throw 1', post_context: [], resolved: true, minified: { filename: 'app.min.js', lineno: 1, colno: 10 } }] }],
  events: [
    { event_id: 'a'.repeat(32), occurred_at: '2023-11-14T11:00:00Z', release: '1.0', environment: 'prod' },
    { event_id: 'b'.repeat(32), occurred_at: '2023-11-14T10:00:00Z', release: '1.0', environment: 'prod' },
  ],
  replay_id: null,
}

describe('Issues/Show', () => {
  it('shows the title, the stack trace, the breadcrumbs and the context', () => {
    const w = mount(Show, { props: props as never })
    expect(w.text()).toContain('TypeError: x is undefined')
    expect(w.find('.stack-trace').exists()).toBe(true)
    expect(w.text()).toContain('pay')
    expect(w.text()).toContain('button#pay')
    expect(w.text()).toContain('jane@example.com')
    expect(w.text()).toContain('https://app.example.com/checkout')
    expect(w.text()).toContain('browser')
    expect(w.text()).toContain('1.0')
  })

  it('lets the user switch between the events of the issue', () => {
    const w = mount(Show, { props: props as never })
    const link = w.find(`a[href="/projects/5/issues/11?event=${'b'.repeat(32)}"]`)
    expect(link.exists()).toBe(true)
  })

  it('changes the status of the issue', async () => {
    const w = mount(Show, { props: props as never })
    await w.find('button.set-resolved').trigger('click')
    expect(router.post).toHaveBeenCalledWith('/projects/5/issues/11/status', { status: 'resolved' }, expect.anything())
  })

  it('offers to reopen a resolved issue', async () => {
    const w = mount(Show, { props: { ...props, issue: { ...props.issue, status: 'resolved' } } as never })
    expect(w.find('button.set-resolved').exists()).toBe(false)
    await w.find('button.set-unresolved').trigger('click')
    expect(router.post).toHaveBeenCalledWith('/projects/5/issues/11/status', { status: 'unresolved' }, expect.anything())
  })

  it('links to the session replay only when there is one', () => {
    expect(mount(Show, { props: props as never }).find('a.replay-link').exists()).toBe(false)
    const rid = 'c'.repeat(32)
    const w = mount(Show, { props: { ...props, replay_id: rid } as never })
    expect(w.find('a.replay-link').attributes('href')).toBe(`/projects/5/replays/${rid}`)
  })
})
