<script setup lang="ts">
const route = useRoute()
await navigateTo(`/nodes/${route.params.id}/detail`, { replace: true })
</script>

<template>
  <div class="text-muted" style="padding: 24px">正在进入节点详情…</div>
</template>
