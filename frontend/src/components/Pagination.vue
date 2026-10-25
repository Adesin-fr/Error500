<script setup lang="ts">
import { Link } from '@inertiajs/vue3'

defineProps<{ page: number; pages: number; url: (page: number) => string }>()
</script>

<template>
  <nav v-if="pages > 1" class="pagination" aria-label="Pagination">
    <Link v-if="page > 1" rel="prev" class="btn" :href="url(page - 1)">← Précédent</Link>
    <span v-else class="btn" aria-disabled="true">← Précédent</span>
    <span class="muted">{{ page }} / {{ pages }}</span>
    <Link v-if="page < pages" rel="next" class="btn" :href="url(page + 1)">Suivant →</Link>
    <span v-else class="btn" aria-disabled="true">Suivant →</span>
  </nav>
</template>

<style scoped>
.pagination { display: flex; gap: 12px; align-items: center; justify-content: center; margin-top: 16px; }
[aria-disabled='true'] { opacity: .5; cursor: not-allowed; }
</style>
