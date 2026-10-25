<script setup lang="ts">
defineProps<{ label: string; error?: string }>()
</script>

<template>
  <div class="field">
    <label>{{ label }}<slot /></label>
    <span v-if="error" class="error-text">{{ error }}</span>
  </div>
</template>
