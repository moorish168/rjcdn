<script setup lang="ts">
const route = useRoute()
const q = String(route.query.domain || '').trim()
if (q) await navigateTo(`/domains/${q}/waf/geo`, { replace: true })
else await navigateTo('/domains', { replace: true })
</script>
<template><div /></template>
