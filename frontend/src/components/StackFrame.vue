<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Frame } from '../types'

const props = defineProps<{ frame: Frame; defaultOpen?: boolean }>()

const open = ref(props.defaultOpen ?? false)
const hasContext = computed(() => props.frame.context_line !== null)

function where(filename: string | null, lineno: number | null, colno: number | null): string {
  return [filename ?? '?', lineno, colno].filter((p) => p !== null).join(':')
}

const location = computed(() => where(props.frame.filename, props.frame.lineno, props.frame.colno))
const minified = computed(() =>
  props.frame.minified ? where(props.frame.minified.filename, props.frame.minified.lineno, props.frame.minified.colno) : null,
)

const lines = computed(() => {
  const { pre_context, context_line, post_context, lineno } = props.frame
  if (context_line === null) return []
  const first = lineno === null ? null : lineno - pre_context.length
  const all = [
    ...pre_context.map((text) => ({ text, error: false })),
    { text: context_line, error: true },
    ...post_context.map((text) => ({ text, error: false })),
  ]
  return all.map((l, i) => ({ ...l, no: first === null ? '' : String(first + i) }))
})
</script>

<template>
  <li class="stack-frame" :class="{ 'is-library': !frame.in_app }">
    <component :is="hasContext ? 'button' : 'div'" class="frame-header" :type="hasContext ? 'button' : undefined"
      :aria-expanded="hasContext ? open : undefined" @click="hasContext && (open = !open)">
      <span class="fn">{{ frame.function ?? '(anonyme)' }}</span>
      <span class="loc mono">{{ location }}</span>
      <span v-if="frame.resolved" class="badge-sourcemap" title="Position remappée par une source map">source map</span>
      <span v-if="minified" class="minified mono muted">← {{ minified }}</span>
    </component>
    <pre v-if="open && hasContext" class="code"><code><span v-for="(l, i) in lines" :key="i" class="code-line"
      :class="{ 'is-error-line': l.error }"><span class="line-no">{{ l.no }}</span><span class="line-text">{{ l.text }}</span>
</span></code></pre>
  </li>
</template>

<style scoped>
.stack-frame { list-style: none; border-bottom: 1px solid var(--border); }
.stack-frame:last-child { border-bottom: 0; }
.frame-header {
  display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; width: 100%;
  padding: 6px 4px; background: none; border: 0; color: inherit; font: inherit; text-align: left;
}
button.frame-header { cursor: pointer; }
button.frame-header:hover { background: var(--surface-2); }
.fn { font-weight: 600; }
.loc { color: var(--muted); }
.is-library .fn, .is-library .loc { opacity: .65; font-weight: 400; }
.badge-sourcemap {
  font-size: 11px; padding: 0 6px; border-radius: 4px; background: var(--accent); color: var(--accent-contrast);
}
.code { margin: 0 0 8px; padding: 8px 0; background: var(--code-bg); border-radius: 6px; overflow-x: auto; }
.code-line { display: flex; white-space: pre; padding: 0 8px; }
.code-line.is-error-line { background: var(--code-line); }
.line-no { width: 3.5em; flex: none; color: var(--muted); text-align: right; padding-right: 12px; user-select: none; }
</style>
