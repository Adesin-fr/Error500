import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, describe, expect, it, vi } from 'vitest'
import CopyField from './CopyField.vue'

afterEach(() => vi.unstubAllGlobals())

describe('CopyField', () => {
  it('shows the value in a read-only field', () => {
    const w = mount(CopyField, { props: { value: 'https://k@host/1', label: 'DSN' } })
    const input = w.find('input').element as HTMLInputElement
    expect(input.value).toBe('https://k@host/1')
    expect(input.readOnly).toBe(true)
    expect(w.text()).toContain('DSN')
  })

  it('copies the value to the clipboard and confirms', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    const w = mount(CopyField, { props: { value: 'secret', label: 'Jeton' } })
    await w.find('button').trigger('click')
    await flushPromises()
    expect(writeText).toHaveBeenCalledWith('secret')
    expect(w.find('button').text()).toContain('Copié')
  })
})
