import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import Breadcrumbs from './Breadcrumbs.vue'

describe('Breadcrumbs', () => {
  it('lists category and message of each breadcrumb', () => {
    const w = mount(Breadcrumbs, {
      props: {
        breadcrumbs: [
          { timestamp: 1700000000, category: 'ui.click', message: 'button#pay', level: 'info' },
          { timestamp: '2023-11-14T22:13:21Z', category: 'console', message: 'about to fail', level: 'warning' },
        ],
      },
    })
    const rows = w.findAll('.breadcrumb')
    expect(rows).toHaveLength(2)
    expect(rows[0]!.text()).toContain('ui.click')
    expect(rows[0]!.text()).toContain('button#pay')
    expect(rows[1]!.classes()).toContain('level-warning')
  })

  it('tolerates breadcrumbs without timestamp, category or message', () => {
    const w = mount(Breadcrumbs, { props: { breadcrumbs: [{}] } })
    expect(w.findAll('.breadcrumb')).toHaveLength(1)
  })

  it('explains when there are none', () => {
    const w = mount(Breadcrumbs, { props: { breadcrumbs: [] } })
    expect(w.text()).toContain('Aucun breadcrumb')
  })
})
