<script setup lang="ts">
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">统计</div>
        <div class="card-desc">查看该站点的流量与请求统计</div>
      </div>
    </div>
    <div class="card-body">
      <p class="text-muted" style="margin-bottom: 14px">
        站点 <strong>{{ domain?.name || id }}</strong> 的统计数据在「监控 → 统计分析」中按域名筛选查看。
      </p>
      <NuxtLink
        :to="`/stats?domain=${encodeURIComponent(domain?.name || id)}`"
        class="btn btn-primary btn-sm"
      >
        打开统计分析
      </NuxtLink>
    </div>
  </div>
</template>
