<script setup lang="ts">
import { certStatusLabel } from '~/utils/certStatus'

const api = useApi()
const toast = useToast()
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
const reloadDomain = inject<() => Promise<void>>('reloadDomain')!

const certs = ref<any[]>([])
const settings = ref<any>(null)
const loading = ref(false)
const refreshing = ref(false)
const issuing = ref(false)
let pollTimer: ReturnType<typeof setInterval> | null = null

const certStatus = computed(
  () => settings.value?.cert_status || domain.value?.cert_status || 'none',
)
const certError = computed(() => settings.value?.cert_last_error || '')
const autoHttps = computed(() => settings.value?.auto_https !== false)
const statusLabel = computed(() => certStatusLabel(certStatus.value))

async function loadCerts() {
  try {
    certs.value = (await api.get<any[]>(`/api/v1/domains/${id.value}/certs`)) || []
  } catch (e: any) {
    certs.value = []
    toast.error(e?.data?.error || e?.message || '加载证书失败')
  }
}

async function loadSettings() {
  try {
    settings.value = await api.get<any>(`/api/v1/domains/${id.value}/settings`)
  } catch (e: any) {
    settings.value = null
    toast.error(e?.data?.error || e?.message || '加载 HTTPS 设置失败')
  }
}

async function load() {
  if (!id.value) return
  loading.value = true
  try {
    await Promise.all([loadCerts(), loadSettings()])
  } finally {
    loading.value = false
  }
  syncPoll()
}

async function refreshStatus() {
  refreshing.value = true
  try {
    await Promise.all([loadSettings(), loadCerts(), reloadDomain?.()])
  } finally {
    refreshing.value = false
  }
  syncPoll()
}

async function retryHttps() {
  issuing.value = true
  try {
    const res = await api.post<any>(`/api/v1/domains/${id.value}/auto-https`, {})
    if (res?.ok) {
      toast.success('已提交申请，证书签发通常需要 1–3 分钟')
      if (settings.value) settings.value.cert_status = 'pending'
    } else {
      toast.error(res?.error || '提交失败')
    }
    await refreshStatus()
  } catch (e: any) {
    toast.error(e?.data?.error || '签发失败')
  } finally {
    issuing.value = false
  }
}

function syncPoll() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
  if (certStatus.value === 'pending') {
    pollTimer = setInterval(() => {
      loadSettings()
      loadCerts()
    }, 8000)
  }
}

watch(id, load, { immediate: true })
onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer)
})
</script>

<template>
  <div class="stack">
    <div class="card">
      <div class="card-head">
        <div>
          <div class="card-title">证书签发状态</div>
          <div class="card-desc">
            CNAME 精确名走 HTTP-01；泛域名需绑定 DNS 服务商。整域托管由平台写 TXT 签发根域与通配证书。
          </div>
        </div>
        <div class="card-extra flex gap-8">
          <button class="btn btn-default btn-sm" type="button" :disabled="refreshing" @click="refreshStatus">
            <AppIcon name="refresh" :size="14" />
            {{ refreshing ? '刷新中…' : '刷新' }}
          </button>
          <button
            class="btn btn-primary btn-sm"
            type="button"
            :disabled="issuing || !autoHttps"
            @click="retryHttps"
          >
            {{ issuing ? '签发中…' : '重试自动 HTTPS' }}
          </button>
        </div>
      </div>
      <div class="card-body">
        <div v-if="loading && !settings" class="text-muted">加载中…</div>
        <template v-else>
          <div class="status-grid">
            <div class="status-item">
              <div class="detail-label">签发状态</div>
              <div class="detail-value">
                <StatusTag :status="certStatus" dot>{{ statusLabel }}</StatusTag>
              </div>
            </div>
            <div class="status-item">
              <div class="detail-label">自动 HTTPS</div>
              <div class="detail-value">{{ autoHttps ? '已开启' : '未开启' }}</div>
            </div>
            <div class="status-item">
              <div class="detail-label">站点 HTTPS</div>
              <div class="detail-value">{{ domain?.ssl_enabled ? '已启用' : '未启用' }}</div>
            </div>
          </div>
          <p v-if="!autoHttps" class="field-hint text-warning mt-12">
            自动 HTTPS 已关闭。可在「高级设置」中开启，或手动上传证书。
          </p>
          <p v-else-if="certStatus === 'pending'" class="field-hint mt-12">
            正在等待 DNS 就绪或 Let's Encrypt 签发，可稍后刷新；签发中会自动轮询。
          </p>
          <p v-else-if="certStatus === 'none'" class="field-hint mt-12">
            尚未开始签发。确认域名已按接入方式解析到平台后，点击「重试自动 HTTPS」。
          </p>
          <div v-if="certError" class="alert alert-danger mt-12">
            <AppIcon name="alert" :size="16" class="icon" />
            <span>{{ certError }}</span>
          </div>
        </template>
      </div>
    </div>

    <div class="card">
      <div class="card-head">
        <div>
          <div class="card-title">已绑定证书</div>
          <div class="card-desc">站点下已签发或上传的证书材料</div>
        </div>
        <div class="card-extra">
          <NuxtLink
            :to="`/certs?domain=${encodeURIComponent(domain?.name || '')}`"
            class="btn btn-default btn-sm"
          >
            上传 / 签发
          </NuxtLink>
        </div>
      </div>
      <div v-if="loading && !certs.length" class="card-body text-muted">加载中…</div>
      <div v-else-if="certs.length" class="table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th>绑定域名</th>
              <th>类型</th>
              <th>签发者</th>
              <th>状态</th>
              <th>有效期至</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="c in certs" :key="c.id">
              <td class="mono">{{ c.domain_name || '—' }}</td>
              <td>
                <StatusTag :tone="(c.cert_kind || '').includes('wild') ? 'info' : 'neutral'">
                  {{ (c.cert_kind || '').includes('wild') ? '泛域名' : '单域名' }}
                </StatusTag>
              </td>
              <td>{{ c.issuer || '—' }}</td>
              <td><StatusTag :status="c.status">{{ c.status }}</StatusTag></td>
              <td class="text-2">{{ formatDay(c.not_after) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <EmptyState
        v-else
        icon="cert"
        title="该站点暂无证书"
        text="自动签发成功后会出现在这里；也可手动上传或签发。"
      >
        <NuxtLink :to="`/certs?domain=${encodeURIComponent(domain?.name || '')}`" class="btn btn-default btn-sm">
          去绑定证书
        </NuxtLink>
      </EmptyState>
    </div>
  </div>
</template>

<style scoped>
.stack {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.status-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px 24px;
}

.detail-label {
  font-size: 12px;
  color: var(--text-3);
}

.detail-value {
  margin-top: 6px;
  font-size: 14px;
  color: var(--text);
}

@media (max-width: 720px) {
  .status-grid {
    grid-template-columns: 1fr;
  }
}
</style>
