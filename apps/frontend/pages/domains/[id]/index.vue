<script setup lang="ts">
const route = useRoute()
await navigateTo(`/domains/${route.params.id}/settings`, { replace: true })
</script>

<template>
  <div class="text-muted" style="padding: 24px">正在进入设置…</div>
</template>
