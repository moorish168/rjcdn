<script setup lang="ts">
import { siteLabel } from '~/utils/siteLabel'

const route = useRoute()
const api = useApi()
const toast = useToast()

const id = computed(() => String(route.params.id || ''))
const domain = ref<any>(null)
const title = computed(() => siteLabel(domain.value) || '站点详情')
const notFound = ref(false)
const loading = ref(false)
const diagOpen = ref(false)
const diagLoading = ref(false)
const diag = ref<any>(null)
const cdnBusy = ref(false)
const { ask } = useConfirm()

provide('domainId', id)
provide('domainRef', domain)
provide('reloadDomain', load)

const cdnHint = computed(() => {
  if (!domain.value) return ''
  const on = domain.value.cdn_enabled !== false
  if (domain.value.access_mode === 'ns') {
    return on
      ? '关闭后 DNS 记录保留，解析改为源站（类似 Cloudflare 灰云）'
      : '开启后解析回到 CDN 节点，边缘回源到你的源站'
  }
  return on
    ? 'CNAME 接入：关闭后仅标记状态；请到注册商把解析改回源站才能真正直回源'
    : 'CNAME 接入：开启后请确保域名 CNAME 指向平台接入地址'
})

async function toggleCdn() {
  if (!domain.value || cdnBusy.value) return
  const next = domain.value.cdn_enabled === false
  const ok = await ask({
    title: next ? '开启 CDN 加速' : '关闭 CDN 加速',
    message: next
      ? domain.value.access_mode === 'ns'
        ? `开启后，${domain.value.name} 的 DNS 将重新解析到 CDN 节点。确认继续？`
        : `开启后请确认域名仍 CNAME 到平台接入地址。确认继续？`
      : domain.value.access_mode === 'ns'
        ? `关闭后 DNS 记录保留，访客将直连源站（不再经过 CDN）。确认继续？`
        : `关闭后平台会标记为「仅 DNS」；CNAME 接入还需你在注册商把解析改回源站。确认继续？`,
    confirmText: next ? '开启加速' : '关闭加速',
    danger: !next,
  })
  if (!ok) return
  cdnBusy.value = true
  try {
    domain.value = await api.post<any>(`/api/v1/domains/${id.value}/cdn`, {
      cdn_enabled: next,
    })
    toast.success(next ? '已开启 CDN 加速' : '已关闭加速，DNS 直回源')
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '切换失败')
  } finally {
    cdnBusy.value = false
  }
}

const SETTINGS_SECTIONS = [
  { key: 'settings', label: '基础信息', to: 'settings' },
  { key: 'hostnames', label: '域名绑定', to: 'hostnames' },
  { key: 'origins', label: '源站', to: 'origins' },
  { key: 'certs', label: 'HTTPS / 证书', to: 'certs' },
  { key: 'rules', label: '缓存', to: 'rules' },
  { key: 'waf', label: 'WAF 防护', to: 'waf' },
  { key: 'advanced', label: '高级设置', to: 'advanced' },
]

const TABS = [
  { key: 'settings', label: '设置', icon: 'settings' },
  { key: 'stats', label: '统计', icon: 'stats' },
  { key: 'logs', label: '访问日志', icon: 'logs' },
  { key: 'delete', label: '删除', icon: 'trash' },
]

const sectionKey = computed(() => {
  const path = route.path
  const base = `/domains/${id.value}`
  if (path === base || path === `${base}/`) return 'settings'
  const rest = path.slice(base.length + 1).split('/')[0] || 'settings'
  return rest
})

const activeTab = computed(() => {
  if (['stats', 'logs', 'delete'].includes(sectionKey.value)) return sectionKey.value
  return 'settings'
})

const showLeftMenu = computed(() => activeTab.value === 'settings')

function tabTo(key: string) {
  if (key === 'settings') return `/domains/${id.value}/settings`
  return `/domains/${id.value}/${key}`
}

function sectionTo(key: string) {
  // WAF lives under nested tabs; land on overview like GoEdge's site WAF home.
  if (key === 'waf') return `/domains/${id.value}/waf/overview`
  return `/domains/${id.value}/${key}`
}

async function load() {
  if (!id.value) return
  loading.value = true
  notFound.value = false
  try {
    domain.value = await api.get<any>(`/api/v1/domains/${id.value}`)
  } catch {
    notFound.value = true
    domain.value = null
  } finally {
    loading.value = false
  }
}

async function runDiagnose() {
  diagOpen.value = true
  diagLoading.value = true
  diag.value = null
  try {
    diag.value = await api.post<any>(`/api/v1/domains/${id.value}/diagnose`, {})
  } catch (e: any) {
    const message = e?.data?.error || '请求失败'
    diag.value = {
      overall: false,
      checks: [{
        key: 'error',
        label: '诊断请求失败',
        ok: false,
        detail: `${message}。请检查控制面/边缘连通性；如果 CNAME 配错，请确认它指向当前统一接入地址，而不是源站或其他 CDN。`,
        latency_ms: 0,
      }],
      resolved_ips: [],
      edge_ips: [],
    }
  } finally {
    diagLoading.value = false
  }
}

watch(id, load, { immediate: true })
</script>

<template>
  <div class="page domain-detail">
    <div class="page-head">
      <div>
        <div class="page-title">
          <NuxtLink to="/domains" class="btn btn-icon btn-default" aria-label="返回网站列表">
            <AppIcon name="arrow-left" :size="16" />
          </NuxtLink>
          <span>{{ title }}</span>
          <StatusTag v-if="domain" :status="domain.status" dot>
            {{ domain.status === 'active' ? '已启用' : '已停用' }}
          </StatusTag>
          <StatusTag
            v-if="domain && domain.status === 'active'"
            :tone="domain.cdn_enabled !== false ? 'info' : 'neutral'"
            :title="cdnHint"
          >
            {{ domain.cdn_enabled !== false ? 'CDN 加速' : '仅 DNS' }}
          </StatusTag>
        </div>
        <div class="page-desc">
          <template v-if="domain?.display_name && domain?.name">主域名 {{ domain.name }} · </template>
          网站设置
        </div>
      </div>
      <div class="page-actions">
        <button
          v-if="domain && domain.status === 'active'"
          class="btn"
          :class="domain.cdn_enabled !== false ? 'btn-primary' : 'btn-default'"
          type="button"
          :disabled="cdnBusy || notFound"
          :title="cdnHint"
          @click="toggleCdn"
        >
          <AppIcon :name="domain.cdn_enabled !== false ? 'zap' : 'globe'" :size="15" />
          {{ cdnBusy ? '切换中…' : domain.cdn_enabled !== false ? '关闭加速' : '开启加速' }}
        </button>
        <button class="btn btn-default" type="button" :disabled="diagLoading || notFound" @click="runDiagnose">
          <AppIcon name="activity" :size="15" />
          {{ diagLoading ? '诊断中…' : '站点诊断' }}
        </button>
        <button class="btn btn-default" type="button" :disabled="loading" @click="load">
          <AppIcon name="refresh" :size="15" />
          刷新
        </button>
      </div>
    </div>

    <div v-if="notFound" class="card">
      <EmptyState icon="domain" title="站点不存在" text="该站点可能已被删除，或你没有访问权限。">
        <NuxtLink to="/domains" class="btn btn-default btn-sm">返回网站列表</NuxtLink>
      </EmptyState>
    </div>

    <div v-else-if="loading && !domain" class="card">
      <div class="card-body text-muted">正在加载站点…</div>
    </div>

    <template v-else>
      <nav class="domain-tabs">
        <NuxtLink
          v-for="tab in TABS"
          :key="tab.key"
          :to="tabTo(tab.key)"
          class="domain-tab"
          :class="{ active: activeTab === tab.key, danger: tab.key === 'delete' }"
        >
          <AppIcon :name="tab.icon" :size="14" />
          {{ tab.label }}
        </NuxtLink>
      </nav>

      <div class="domain-body" :class="{ 'with-left': showLeftMenu }">
        <aside v-if="showLeftMenu" class="domain-left">
          <NuxtLink
            v-for="sec in SETTINGS_SECTIONS"
            :key="sec.key"
            :to="sectionTo(sec.to)"
            class="domain-left-item"
            :class="{ active: sectionKey === sec.key }"
          >
            <AppIcon name="chevron-right" :size="11" class="chev" :class="{ on: sectionKey === sec.key }" />
            {{ sec.label }}
          </NuxtLink>
        </aside>

        <div class="domain-main">
          <NuxtPage />
        </div>
      </div>
    </template>

    <BaseModal v-model="diagOpen" title="域名诊断" width="760px" hide-footer>
      <div v-if="diagLoading" class="text-muted">正在检查 DNS、证书、源站和缓存…</div>
      <template v-else-if="diag">
        <div class="alert" :class="diag.overall ? 'alert-success' : 'alert-warning'">
          <AppIcon :name="diag.overall ? 'check' : 'alert'" :size="16" class="icon" />
          <span>{{ diag.overall ? '域名接入检查通过' : '存在需要处理的接入问题' }}</span>
        </div>
        <div class="diag-grid">
          <div v-for="c in diag.checks" :key="c.key" class="diag-card" :class="{ bad: !c.ok }">
            <div class="diag-head">
              <AppIcon :name="c.ok ? 'check' : 'close'" :size="16" />
              <strong>{{ c.label }}</strong>
              <span class="text-muted">{{ c.latency_ms }} ms</span>
            </div>
            <div class="diag-detail">{{ c.detail }}</div>
          </div>
        </div>
        <div v-if="diag.resolved_ips?.length" class="diag-meta">
          <span>DNS 解析：</span><code>{{ diag.resolved_ips.join(', ') }}</code>
          <span v-if="diag.edge_ips?.length"> · 在线节点：<code>{{ diag.edge_ips.join(', ') }}</code></span>
        </div>
      </template>
    </BaseModal>
  </div>
</template>

<style scoped>
.domain-tabs {
  display: flex;
  gap: 2px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--border);
}

.domain-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 10px 16px;
  color: var(--text-2);
  font-size: 13.5px;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  text-decoration: none;
}

.domain-tab:hover {
  color: var(--text);
}

.domain-tab.active {
  color: var(--brand);
  border-bottom-color: var(--brand);
  font-weight: 500;
}

.domain-tab.danger.active {
  color: var(--danger);
  border-bottom-color: var(--danger);
}

.domain-body {
  display: block;
}

.domain-body.with-left {
  display: grid;
  grid-template-columns: 180px minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}

.domain-left {
  position: sticky;
  top: 12px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 0;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg, 10px);
  background: var(--card);
}

.domain-left-item {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 9px 12px;
  color: var(--text-2);
  font-size: 13px;
  text-decoration: none;
}

.domain-left-item:hover {
  color: var(--text);
  background: var(--bg);
}

.domain-left-item.active {
  color: var(--brand);
  background: color-mix(in srgb, var(--brand) 8%, var(--card));
  font-weight: 500;
}

.domain-left-item .chev {
  opacity: 0;
  flex: none;
}

.domain-left-item .chev.on,
.domain-left-item.active .chev {
  opacity: 1;
}

.domain-main {
  min-width: 0;
}

.diag-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-top: 12px;
}

.diag-card {
  border: 1px solid var(--success-border);
  background: var(--success-bg);
  border-radius: var(--radius);
  padding: 12px;
}

.diag-card.bad {
  border-color: var(--danger-border);
  background: var(--danger-bg);
}

.diag-head {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--text);
  font-size: 13px;
}

.diag-head .text-muted {
  margin-left: auto;
  font-size: 11px;
}

.diag-detail {
  margin-top: 6px;
  color: var(--text-2);
  font-size: 12px;
  line-height: 1.6;
  word-break: break-all;
}

.diag-meta {
  margin-top: 12px;
  color: var(--text-3);
  font-size: 12px;
  word-break: break-all;
}

@media (max-width: 860px) {
  .domain-body.with-left {
    grid-template-columns: 1fr;
  }

  .domain-left {
    position: static;
    flex-direction: row;
    flex-wrap: wrap;
    padding: 6px;
  }

  .diag-grid {
    grid-template-columns: 1fr;
  }
}
</style>
