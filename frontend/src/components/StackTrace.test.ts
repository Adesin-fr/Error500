import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { ExceptionView, Frame } from '../types'
import StackTrace from './StackTrace.vue'

const frame = (fn: string, in_app: boolean): Frame => ({
  filename: `${fn}.js`,
  function: fn,
  lineno: 1,
  colno: 1,
  in_app,
  pre_context: [],
  context_line: `${fn}()`,
  post_context: [],
  resolved: false,
  minified: null,
})

const exceptions: ExceptionView[] = [
  { type: 'TypeError', value: 'x is undefined', frames: [frame('mine', true), frame('lib', false), frame('mine2', true)] },
  { type: 'Error', value: 'cause', frames: [frame('inner', true)] },
]

describe('StackTrace', () => {
  it('shows each exception with its type and value', () => {
    const w = mount(StackTrace, { props: { exceptions } })
    const titles = w.findAll('.exception-title').map((t) => t.text())
    expect(titles).toEqual(['TypeError: x is undefined', 'Error: cause'])
  })

  it('shows every frame by default', () => {
    const w = mount(StackTrace, { props: { exceptions } })
    expect(w.findAll('.stack-frame')).toHaveLength(4)
  })

  it('can restrict the view to application frames', async () => {
    const w = mount(StackTrace, { props: { exceptions } })
    await w.find('input[type="checkbox"]').setValue(true)
    expect(w.findAll('.stack-frame')).toHaveLength(3)
    expect(w.text()).not.toContain('lib.js')
  })

  it('opens the first application frame of the first exception', () => {
    const w = mount(StackTrace, { props: { exceptions } })
    expect(w.findAll('.code')).toHaveLength(1)
    expect(w.find('.code').text()).toContain('mine()')
  })

  it('explains when there is no stack trace', () => {
    const w = mount(StackTrace, { props: { exceptions: [] } })
    expect(w.text()).toContain('Aucune stack trace')
  })

  it('handles an exception without frames', () => {
    const w = mount(StackTrace, { props: { exceptions: [{ type: 'E', value: 'v', frames: [] }] } })
    expect(w.text()).toContain('E: v')
    expect(w.text()).toContain('Pas de frames')
  })
})
