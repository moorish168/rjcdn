<script setup lang="ts">
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
const api = useApi()
const toast = useToast()

const enabled = ref(true)
const loading = ref(true)
const saving = ref(false)

async function load() {
  loading.value = true
  try {
    const s = await api.get<any>(`/api/v1/domains/${id.value}/settings`)
    enabled.value = s.access_log_enabled !== false
  } catch {
    enabled.value = true
  } finally {
    loading.value = false
  }
}

async function save() {
  saving.value = true
  try {
    await api.put(`/api/v1/domains/${id.value}/settings`, {
      access_log_enabled: enabled.value,
    })
    toast.success(enabled.value ? '已开启访问日志，边缘约 15 秒内生效' : '已关闭访问日志，边缘约 15 秒内生效')
  } catch (e: any) {
    toast.error(e?.data?.error || '保存失败')
    await load()
  } finally {
    saving.value = false
  }
}

watch(id, load, { immediate: true })
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">访问日志</div>
        <div class="card-desc">按站点决定要不要记录。关闭后新的请求不再写入</div>
      </div>
    </div>
    <div class="card-body">
      <label class="check">
        <input v-model="enabled" type="checkbox" :disabled="loading || saving" @change="save" />
        记录 {{ domain?.name || '此站点' }} 的访问日志
      </label>
      <p class="text-muted" style="margin: 14px 0">
        已记录的日志仍在「网站 → 访问日志」里。关闭只影响之后的新请求。
      </p>
      <NuxtLink
        :to="`/access-logs?domain=${encodeURIComponent(domain?.name || id)}`"
        class="btn btn-primary btn-sm"
      >
        打开访问日志
      </NuxtLink>
    </div>
  </div>
</template>
