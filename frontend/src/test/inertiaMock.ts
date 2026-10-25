// Doublure minimale de @inertiajs/vue3 pour tester les pages sans serveur.
import { defineComponent, reactive } from 'vue'
import { vi } from 'vitest'

export const state = {
  props: { auth: { user: { id: 1, name: 'Alice', email: 'alice@example.com', is_admin: false } } } as Record<string, unknown>,
}

export const router = { get: vi.fn(), post: vi.fn(), delete: vi.fn(), visit: vi.fn() }

export const forms: Array<Record<string, any>> = []

export const mock = {
  Link: defineComponent({ props: ['href', 'method', 'as'], template: '<a :href="href"><slot /></a>' }),
  Head: defineComponent({ template: '<span />' }),
  usePage: () => ({ props: state.props }),
  router,
  useForm: (data: Record<string, unknown>) => {
    const form = reactive({
      ...data,
      errors: {} as Record<string, string>,
      processing: false,
      post: vi.fn(),
      delete: vi.fn(),
      reset: vi.fn(),
    })
    forms.push(form)
    return form
  },
}

export function resetInertiaMock() {
  router.get.mockReset()
  router.post.mockReset()
  router.delete.mockReset()
  router.visit.mockReset()
  forms.length = 0
  state.props = { auth: { user: { id: 1, name: 'Alice', email: 'alice@example.com', is_admin: false } } }
}
