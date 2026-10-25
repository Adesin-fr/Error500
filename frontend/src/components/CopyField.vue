<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{ value: string; label: string }>()
const copied = ref(false)

async function copy() {
  await navigator.clipboard.writeText(props.value)
  copied.value = true
  setTimeout(() => (copied.value = false), 2000)
}
</script>

<template>
  <div class="field">
    <label>{{ label }}</label>
    <div class="row">
      <input class="input mono grow" readonly :value="value" @focus="($event.target as HTMLInputElement).select()" />
      <button type="button" class="btn" @click="copy">{{ copied ? 'Copié ✓' : 'Copier' }}</button>
    </div>
  </div>
</template>

<style scoped>
.grow { flex: 1; }
</style>
