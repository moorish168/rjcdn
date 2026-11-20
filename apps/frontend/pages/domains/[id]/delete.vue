<script setup lang="ts">
const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
const busy = ref(false)

async function remove() {
  const name = domain.value?.name || id.value
  const ok = await ask({
    title: '删除站点',
    message: `删除后 ${name} 及其全部域名将不再走 CDN，相关缓存规则会被一并删除。此操作不可恢复。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  busy.value = true
  try {
    await api.del(`/api/v1/domains/${id.value}`)
    toast.success('站点已删除')
    await navigateTo('/domains')
  } catch (e: any) {
    toast.error(e?.data?.error || '删除失败')
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">删除站点</div>
        <div class="card-desc">危险操作，删除后不可恢复</div>
      </div>
    </div>
    <div class="card-body">
      <div class="alert alert-danger" role="alert">
        <AppIcon name="alert" :size="16" class="icon" />
        <span>
          将永久删除 <strong>{{ domain?.name || id }}</strong> 及其域名绑定、缓存规则关联。边缘约 15 秒内停止加速。
        </span>
      </div>
      <button class="btn btn-danger" type="button" :disabled="busy" @click="remove">
        {{ busy ? '删除中…' : '确认删除此站点' }}
      </button>
    </div>
  </div>
</template>
