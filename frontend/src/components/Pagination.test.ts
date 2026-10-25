import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import Pagination from './Pagination.vue'

const Link = { props: ['href'], template: '<a :href="href"><slot /></a>' }
const mountIt = (page: number, pages: number) =>
  mount(Pagination, { props: { page, pages, url: (p: number) => `/list?page=${p}` }, global: { stubs: { Link } } })

describe('Pagination', () => {
  it('is hidden when there is a single page', () => {
    expect(mountIt(1, 1).find('nav').exists()).toBe(false)
  })

  it('links to the previous and next pages', () => {
    const w = mountIt(2, 3)
    expect(w.text()).toContain('2 / 3')
    expect(w.find('a[rel="prev"]').attributes('href')).toBe('/list?page=1')
    expect(w.find('a[rel="next"]').attributes('href')).toBe('/list?page=3')
  })

  it('disables the previous link on the first page', () => {
    const w = mountIt(1, 3)
    expect(w.find('a[rel="prev"]').exists()).toBe(false)
    expect(w.find('[aria-disabled="true"]').text()).toContain('Précédent')
  })

  it('disables the next link on the last page', () => {
    const w = mountIt(3, 3)
    expect(w.find('a[rel="next"]').exists()).toBe(false)
  })
})
