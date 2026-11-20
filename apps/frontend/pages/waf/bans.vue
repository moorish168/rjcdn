<script setup lang="ts">
const route = useRoute()
const q = String(route.query.domain || '').trim()
if (q) await navigateTo(`/domains/${q}/waf/bans`, { replace: true })
else await navigateTo('/domains', { replace: true })
</script>
<template><div /></template>
