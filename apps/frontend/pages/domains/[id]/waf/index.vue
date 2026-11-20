<script setup lang="ts">
const id = inject<Ref<string>>('domainId')!
await navigateTo(`/domains/${id.value}/waf/overview`, { replace: true })
</script>

<template>
  <div />
</template>
