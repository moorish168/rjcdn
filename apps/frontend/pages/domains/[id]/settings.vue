<script setup lang="ts">
import { certStatusLabel } from '~/utils/certStatus'

const api = useApi()
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
const reloadDomain = inject<() => Promise<void>>('reloadDomain')!

const toast = useToast()
const { ask } = useConfirm()
const projects = ref<any[]>([])
const settings = ref<any>(null)
const loading = ref(false)
const savingName = ref(false)
const cdnBusy = ref(false)
const displayName = ref('')

const projectName = computed(() => {
  if (!domain.value?.project_id) return ''
  const p = projects.value.find((x) => x.id === domain.value.project_id)
  return p?.name || ''
})

const certStatus = computed(
  () => settings.value?.cert_status || domain.value?.cert_status || 'none',
)
const certLabel = computed(() => certStatusLabel(certStatus.value))
const certError = computed(() => settings.value?.cert_last_error || '')

const nameDirty = computed(
  () => displayName.value.trim() !== String(domain.value?.display_name || '').trim(),
)

watch(
  () => domain.value?.id,
  () => {
    displayName.value = String(domain.value?.display_name || '')
  },
  { immediate: true },
)

async function saveDisplayName() {
  if (savingName.value || !nameDirty.value) return
  savingName.value = true
  try {
    await api.put(`/api/v1/domains/${id.value}`, {
      display_name: displayName.value.trim(),
    })
    toast.success('站点名称已保存')
    await reloadDomain?.()
    displayName.value = String(domain.value?.display_name || '')
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '保存失败')
  } finally {
    savingName.value = false
  }
}

async function toggleCdn() {
  if (cdnBusy.value || domain.value?.status !== 'active') return
  const next = domain.value?.cdn_enabled === false
  const ok = await ask({
    title: next ? '开启 CDN 加速' : '关闭 CDN 加速',
    message: next
      ? '开启后解析将重新指向 CDN。确认继续？'
      : domain.value?.access_mode === 'ns'
        ? '关闭后 DNS 记录保留，访客直连源站。确认继续？'
        : '关闭后还需在注册商把解析改回源站。确认继续？',
    confirmText: next ? '开启加速' : '关闭加速',
    danger: !next,
  })
  if (!ok) return
  cdnBusy.value = true
  try {
    const row = await api.post<any>(`/api/v1/domains/${id.value}/cdn`, {
      cdn_enabled: next,
    })
    if (domain.value) Object.assign(domain.value, row)
    toast.success(next ? '已开启 CDN 加速' : '已关闭加速，DNS 直回源')
    await reloadDomain?.()
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '切换失败')
  } finally {
    cdnBusy.value = false
  }
}

async function refresh() {
  loading.value = true
  try {
    await reloadDomain?.()
    settings.value = await api.get<any>(`/api/v1/domains/${id.value}/settings`)
    displayName.value = String(domain.value?.display_name || '')
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '刷新失败')
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  loading.value = true
  try {
    const [p, s] = await Promise.all([
      api.get<any[]>('/api/v1/projects').catch(() => []),
      api.get<any>(`/api/v1/domains/${id.value}/settings`),
    ])
    projects.value = p || []
    settings.value = s
    displayName.value = String(domain.value?.display_name || '')
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '加载基础信息失败')
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div class="card-title">基础信息</div>
      <div class="card-extra">
        <button class="btn btn-default btn-sm" type="button" :disabled="loading" @click="refresh">
          <AppIcon name="refresh" :size="14" />
          刷新
        </button>
      </div>
    </div>
    <div v-if="loading && !settings && !domain" class="card-body text-muted">正在加载…</div>
    <div v-else class="card-body">
      <div class="detail-grid">
        <div class="detail-item detail-item-wide">
          <div class="detail-label">站点名称</div>
          <div class="name-edit">
            <input
              v-model="displayName"
              class="input"
              maxlength="64"
              placeholder="给站点起个好记的名字，可留空"
              @keyup.enter="saveDisplayName"
            />
            <button
              class="btn btn-primary btn-sm"
              type="button"
              :disabled="savingName || !nameDirty"
              @click="saveDisplayName"
            >
              {{ savingName ? '保存中…' : '保存' }}
            </button>
          </div>
          <div class="field-hint">仅用于控制台展示，不影响域名解析与加速。留空则显示主域名。</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">主域名</div>
          <div class="detail-value mono">{{ domain?.name || '—' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">源站地址</div>
          <div class="detail-value mono">
            {{ domain ? `${domain.origin_proto}://${domain.origin_host}:${domain.origin_port}` : '—' }}
          </div>
        </div>
        <div class="detail-item">
          <div class="detail-label">HTTPS</div>
          <div class="detail-value">{{ domain?.ssl_enabled ? '已启用' : '未启用' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">证书签发</div>
          <div class="detail-value">
            <StatusTag :status="certStatus" dot>{{ certLabel }}</StatusTag>
            <NuxtLink :to="`/domains/${id}/certs`" class="cert-link">详情</NuxtLink>
          </div>
        </div>
        <div class="detail-item">
          <div class="detail-label">限流</div>
          <div class="detail-value">
            {{
              settings?.rate_limit_rps || domain?.rate_limit_rps
                ? `${settings?.rate_limit_rps || domain?.rate_limit_rps} QPS`
                : '不限'
            }}
          </div>
        </div>
        <div class="detail-item">
          <div class="detail-label">所属项目</div>
          <div class="detail-value">{{ projectName || '未归属项目' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">域名 ID</div>
          <div class="detail-value mono">{{ domain?.id || '—' }}</div>
        </div>
        <div class="detail-item detail-item-wide">
          <div class="detail-label">CDN 加速</div>
          <div class="cdn-row">
            <label
              class="switch"
              :class="{ on: domain?.cdn_enabled !== false, disabled: cdnBusy || domain?.status !== 'active' }"
              @click.prevent="toggleCdn"
            >
              <span class="switch-track" />
            </label>
            <div class="cdn-copy">
              <strong>{{ domain?.cdn_enabled !== false ? '已开启（橙云）' : '已关闭（灰云 / 仅 DNS）' }}</strong>
              <span class="field-hint">
                <template v-if="domain?.access_mode === 'ns'">
                  关闭后 DNS 记录保留，解析改为源站；开启后重新走 CDN。
                </template>
                <template v-else>
                  CNAME 接入下开关只改站点标记；真正直回源需在注册商改解析。
                </template>
              </span>
            </div>
          </div>
        </div>
      </div>

      <div v-if="certError" class="alert alert-danger mt-16">
        <AppIcon name="alert" :size="16" class="icon" />
        <span>证书签发：{{ certError }}</span>
      </div>

      <div class="tips">
        <div class="tip-row">
          <AppIcon
            :name="domain?.status === 'active' ? 'check' : 'close'"
            :size="15"
            :class="domain?.status === 'active' ? 'text-success' : 'text-muted'"
          />
          <span>域名状态为启用</span>
        </div>
        <div class="tip-row">
          <AppIcon name="info" :size="15" class="text-muted" />
          <span>源站、证书、WAF、高级参数请使用左侧菜单进入对应设置页</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 18px 24px;
}

.detail-item-wide {
  grid-column: 1 / -1;
}

.name-edit {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 4px;
}

.name-edit .input {
  flex: 1;
  min-width: 0;
}

.field-hint {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-3);
}

.detail-label {
  font-size: 12px;
  color: var(--text-3);
}

.detail-value {
  margin-top: 4px;
  font-size: 14px;
  color: var(--text);
  word-break: break-all;
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.cert-link {
  font-size: 12px;
  color: var(--brand);
}

.cdn-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  margin-top: 6px;
}

.cdn-copy {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.cdn-copy strong {
  font-size: 14px;
  font-weight: 600;
}

.switch.disabled {
  opacity: 0.45;
  pointer-events: none;
}

.tips {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 20px;
  padding-top: 16px;
  border-top: 1px solid var(--border);
}

.tip-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-2);
}

@media (max-width: 640px) {
  .detail-grid {
    grid-template-columns: 1fr;
  }
}
</style>
