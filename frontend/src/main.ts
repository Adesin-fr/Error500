import { createInertiaApp } from '@inertiajs/vue3'
import { createApp, h, type DefineComponent } from 'vue'
import './styles.css'

const pages = import.meta.glob<DefineComponent>('./pages/**/*.vue', { eager: true })

createInertiaApp({
  title: (title) => (title ? `${title} · error500` : 'error500'),
  resolve: (name) => {
    const page = pages[`./pages/${name}.vue`]
    if (!page) throw new Error(`Page Inertia introuvable : ${name}`)
    return page
  },
  setup({ el, App, props, plugin }) {
    createApp({ render: () => h(App, props) })
      .use(plugin)
      .mount(el)
  },
})
