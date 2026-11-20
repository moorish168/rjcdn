<script setup lang="ts">
const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
const reloadDomain = inject<() => Promise<void>>('reloadDomain')!

const newHostname = ref('')
const hostnameError = ref('')
const hostnameBusy = ref(false)

const hostnameDetails = computed(() => {
  if (domain.value?.hostname_details?.length) return domain.value.hostname_details
  if (domain.value?.hostnames?.length) {
    return domain.value.hostnames.map((hostname: string, i: number) => ({
      id: '',
      hostname,
      is_primary: i === 0,
    }))
  }
  return domain.value?.name
    ? [{ id: '', hostname: domain.value.name, is_primary: true }]
    : []
})

function isValidHostname(raw: string): boolean {
  const name = raw.trim().toLowerCase().replace(/\.$/, '')
  if (!name || name.length > 253) return false
  if (name.startsWith('*.')) {
    return /^\*\.([a-z0-9]([a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}$/i.test(name)
  }
  if (name.includes('*')) return false
  return /^([a-z0-9]([a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}$/i.test(name)
}

async function addHostname() {
  hostnameError.value = ''
  const hostname = newHostname.value.trim().toLowerCase().replace(/\.$/, '')
  if (!hostname) {
    hostnameError.value = '请填写域名'
    return
  }
  if (!isValidHostname(hostname)) {
    hostnameError.value = '域名格式无效；泛域名仅支持 *.example.com 这种前导单标签形式'
    return
  }
  hostnameBusy.value = true
  try {
    await api.post(`/api/v1/domains/${id.value}/hostnames`, { hostname })
    newHostname.value = ''
    toast.success('域名已添加，边缘约 15 秒内同步')
    await reloadDomain()
  } catch (e: any) {
    hostnameError.value = e?.data?.error || '添加失败'
  } finally {
    hostnameBusy.value = false
  }
}

async function removeHostname(h: any) {
  if (!h?.id) return
  const ok = await ask({
    title: '移除域名绑定',
    message: `确定移除「${h.hostname}」？边缘节点将在约 15 秒内停止接受该主机名。`,
    confirmText: '移除',
    danger: true,
  })
  if (!ok) return
  hostnameBusy.value = true
  hostnameError.value = ''
  try {
    await api.del(`/api/v1/domains/${id.value}/hostnames/${h.id}`)
    toast.success('域名已移除')
    await reloadDomain()
  } catch (e: any) {
    hostnameError.value = e?.data?.error || '移除失败'
  } finally {
    hostnameBusy.value = false
  }
}
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">域名绑定</div>
        <div class="card-desc">
          同一站点可绑定多个域名；支持 <code>*.example.com</code>。精确名优先于一级泛匹配。
        </div>
      </div>
    </div>
    <div class="card-body">
      <div class="hostname-list">
        <div v-for="h in hostnameDetails" :key="h.id || h.hostname" class="hostname-row">
          <div>
            <div class="mono">{{ h.hostname }}</div>
            <div class="cell-sub">
              {{ h.is_primary ? '主域名' : '别名' }}
              <template v-if="String(h.hostname).startsWith('*.')"> · 泛域名</template>
            </div>
          </div>
          <button
            v-if="!h.is_primary && h.id"
            class="btn btn-text btn-sm danger"
            type="button"
            :disabled="hostnameBusy"
            @click="removeHostname(h)"
          >
            移除
          </button>
        </div>
      </div>
      <div class="hostname-add">
        <input
          v-model="newHostname"
          class="input"
          placeholder="再添加一个域名，如 www.example.com 或 *.example.com"
        />
        <button class="btn btn-default btn-sm" type="button" :disabled="hostnameBusy" @click="addHostname">
          添加
        </button>
      </div>
      <p v-if="hostnameError" class="text-danger" style="font-size: 13px; margin-top: 8px">{{ hostnameError }}</p>
    </div>
  </div>
</template>

<style scoped>
.hostname-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.hostname-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg);
}

.hostname-add {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.hostname-add .input {
  flex: 1;
}
</style>
