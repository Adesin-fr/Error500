import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { Frame } from '../types'
import StackFrame from './StackFrame.vue'

const base: Frame = {
  filename: 'src/app.ts',
  function: 'handleClick',
  lineno: 12,
  colno: 5,
  in_app: true,
  pre_context: ['const a = 1', 'const b = 2'],
  context_line: 'throw new Error(a + b)',
  post_context: ['}'],
  resolved: false,
  minified: null,
}

describe('StackFrame', () => {
  it('shows the function and its location', () => {
    const w = mount(StackFrame, { props: { frame: base } })
    expect(w.text()).toContain('handleClick')
    expect(w.text()).toContain('src/app.ts:12:5')
  })

  it('falls back to a placeholder for anonymous functions', () => {
    const w = mount(StackFrame, { props: { frame: { ...base, function: null } } })
    expect(w.text()).toContain('(anonyme)')
  })

  it('hides the code context until the frame is opened', async () => {
    const w = mount(StackFrame, { props: { frame: base } })
    expect(w.find('.code').exists()).toBe(false)
    await w.find('button.frame-header').trigger('click')
    expect(w.find('.code').exists()).toBe(true)
  })

  it('opens the context by default when asked to', () => {
    const w = mount(StackFrame, { props: { frame: base, defaultOpen: true } })
    expect(w.find('.code').exists()).toBe(true)
  })

  it('numbers the lines around the failing one and highlights it', () => {
    const w = mount(StackFrame, { props: { frame: base, defaultOpen: true } })
    const rows = w.findAll('.code-line')
    expect(rows.map((r) => r.find('.line-no').text())).toEqual(['10', '11', '12', '13'])
    const failing = w.findAll('.code-line.is-error-line')
    expect(failing).toHaveLength(1)
    expect(failing[0]!.text()).toContain('throw new Error(a + b)')
  })

  it('has nothing to open when there is no context', () => {
    const w = mount(StackFrame, { props: { frame: { ...base, pre_context: [], context_line: null, post_context: [] } } })
    expect(w.find('button.frame-header').exists()).toBe(false)
    expect(w.text()).toContain('handleClick')
  })

  it('tells when the frame was mapped to the original source', () => {
    const frame: Frame = {
      ...base,
      resolved: true,
      minified: { filename: 'https://app.example.com/app.min.js', lineno: 1, colno: 10 },
    }
    const w = mount(StackFrame, { props: { frame } })
    expect(w.find('.badge-sourcemap').exists()).toBe(true)
    expect(w.text()).toContain('app.min.js:1:10')
  })

  it('marks frames from libraries as not in app', () => {
    const w = mount(StackFrame, { props: { frame: { ...base, in_app: false } } })
    expect(w.classes()).toContain('is-library')
  })
})
