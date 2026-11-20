<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">网站列表</div>
        <div class="page-desc">管理加速站点；源站、证书、WAF 等请进入编辑页配置</div>
      </div>
      <div class="page-actions">
        <button class="btn btn-default" type="button" @click="exportDomains">
          <AppIcon name="upload" :size="15" />
          导出 CSV
        </button>
        <button class="btn btn-default" type="button" :disabled="loading" @click="load">
          <AppIcon name="refresh" :size="15" />
          刷新
        </button>
        <NuxtLink to="/domains/onboard" class="btn btn-primary">
          <AppIcon name="plus" :size="15" />
          添加站点
        </NuxtLink>
      </div>
    </div>

    <div class="card">
      <div class="filter-bar">
        <div class="search-box">
          <AppIcon name="search" :size="15" class="search-icon" />
          <input
            v-model="keyword"
            class="input"
            placeholder="搜索站点名称、域名或源站"
            style="padding-left: 32px; min-width: 220px"
          />
        </div>
        <select v-model="statusFilter" class="select">
          <option value="">全部状态</option>
          <option value="active">已启用</option>
          <option value="disabled">已停用</option>
        </select>
        <select v-model="projectFilter" class="select">
          <option value="">全部项目</option>
          <option value="none">未归属项目</option>
          <option v-for="p in projects" :key="p.id" :value="p.id">{{ projectLabel(p) }}</option>
        </select>
        <span class="spacer" />
        <span class="text-muted" style="font-size: 13px">共 {{ filtered.length }} 个站点</span>
      </div>

      <div v-if="loading" class="card-body">
        <div class="skeleton" style="height: 14px; width: 40%; margin-bottom: 12px" />
        <div class="skeleton" style="height: 34px; width: 100%; margin-bottom: 8px" />
        <div class="skeleton" style="height: 34px; width: 100%; margin-bottom: 8px" />
        <div class="skeleton" style="height: 34px; width: 100%" />
      </div>

      <template v-else-if="paged.length">
        <div v-if="selected.length" class="bulk-bar">
          <span>已选择 <b>{{ selected.length }}</b> 个站点</span>
          <button class="btn btn-text btn-sm" type="button" :disabled="bulkBusy" @click="bulkStatus('active')">
            批量启用
          </button>
          <button class="btn btn-text btn-sm" type="button" :disabled="bulkBusy" @click="bulkStatus('disabled')">
            批量停用
          </button>
          <button class="btn btn-text btn-sm danger" type="button" :disabled="bulkBusy" @click="bulkDelete">
            批量删除
          </button>
          <button class="btn btn-text btn-sm" type="button" :disabled="bulkBusy" @click="selected = []">取消</button>
        </div>
        <div class="table-wrap">
          <table class="table">
            <thead>
              <tr>
                <th class="check-col">
                  <input type="checkbox" :checked="allPageSelected" @change="togglePage" />
                </th>
                <th style="min-width: 200px">站点 / 域名</th>
                <th style="min-width: 180px">源站</th>
                <th style="min-width: 140px">所属项目</th>
                <th class="switch-col">加速</th>
                <th class="switch-col">缓存</th>
                <th class="switch-col">启用</th>
                <th class="ops-col">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="d in paged" :key="d.id" :class="{ 'row-off': d.status !== 'active' }">
                <td class="check-col">
                  <input v-model="selected" type="checkbox" :value="d.id" />
                </td>
                <td>
                  <NuxtLink :to="`/domains/${d.id}/settings`" class="cell-main">{{ siteLabel(d) }}</NuxtLink>
                  <div class="cell-sub">
                    <template v-if="d.display_name">
                      {{ d.name }}
                      <template v-if="(d.hostnames?.length || 1) > 1">
                        · {{ d.hostnames.length }} 个域名
                        <span v-if="aliasPreview(d)"> · {{ aliasPreview(d) }}</span>
                      </template>
                    </template>
                    <template v-else-if="(d.hostnames?.length || 1) > 1">
                      {{ d.hostnames.length }} 个域名
                      <span v-if="aliasPreview(d)"> · {{ aliasPreview(d) }}</span>
                    </template>
                    <template v-else>
                      {{ d.ssl_enabled ? 'HTTPS' : 'HTTP' }}
                    </template>
                  </div>
                </td>
                <td>
                  <div class="mono text-2 origin-cell">
                    {{ d.origin_proto }}://{{ d.origin_host }}:{{ d.origin_port }}
                  </div>
                  <div v-if="d.origins?.length > 1" class="cell-sub">{{ d.origins.length }} 个源站</div>
                </td>
                <td>
                  <select
                    class="select proj-select"
                    :value="d.project_id || ''"
                    :disabled="!auth.isAdmin && !projects.length"
                    @change="onProjectChange(d, ($event.target as HTMLSelectElement).value)"
                  >
                    <option v-if="auth.isAdmin" value="">未归属</option>
                    <option v-for="p in projects" :key="p.id" :value="p.id">{{ projectLabel(p) }}</option>
                  </select>
                </td>
                <td class="switch-col">
                  <label
                    class="switch"
                    :class="{
                      on: d.cdn_enabled !== false && d.status === 'active',
                      disabled: isBusy(d.id) || d.status !== 'active',
                    }"
                    :title="cdnTitle(d)"
                    @click.prevent="toggleCdn(d)"
                  >
                    <span class="switch-track" />
                  </label>
                </td>
                <td class="switch-col">
                  <label
                    class="switch"
                    :class="{
                      on: d.cache_enabled !== false && d.status === 'active',
                      disabled: isBusy(d.id) || d.status !== 'active',
                    }"
                    :title="cacheTitle(d)"
                    @click.prevent="toggleCache(d)"
                  >
                    <span class="switch-track" />
                  </label>
                </td>
                <td class="switch-col">
                  <label
                    class="switch"
                    :class="{ on: d.status === 'active', disabled: isBusy(d.id) }"
                    :title="d.status === 'active' ? '站点已启用' : '站点已停用'"
                    @click.prevent="toggleStatus(d)"
                  >
                    <span class="switch-track" />
                  </label>
                </td>
                <td class="ops-col">
                  <div class="ops">
                    <NuxtLink :to="`/domains/${d.id}/settings`" class="btn btn-text btn-sm">编辑</NuxtLink>
                    <NuxtLink :to="`/domains/${d.id}/delete`" class="btn btn-text btn-sm danger">删除</NuxtLink>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <div class="table-footer">
          <AppPagination v-model:page="page" :total="filtered.length" :page-size="pageSize" />
        </div>
      </template>

      <EmptyState
        v-else
        icon="domain"
        :title="domains.length ? '没有匹配的站点' : '还没有站点'"
        :text="
          domains.length
            ? '试试调整搜索关键词或筛选条件。'
            : '通过向导添加站点并配置源站后，边缘节点会自动同步并开始加速。'
        "
      >
        <NuxtLink v-if="!domains.length" to="/domains/onboard" class="btn btn-primary btn-sm">
          添加第一个站点
        </NuxtLink>
      </EmptyState>
    </div>
  </div>
</template>

<script setup lang="ts">
import { siteLabel } from '~/utils/siteLabel'

const api = useApi()
const auth = useAuthStore()
const toast = useToast()
const { ask } = useConfirm()

const loading = ref(false)
const domains = ref<any[]>([])
const projects = ref<any[]>([])
const busyIds = ref<Set<string>>(new Set())

function projectLabel(project: any) {
  const owner = project?.owner_email || project?.owner_name || ''
  return owner ? `${project.name}（${owner}）` : project.name
}

const keyword = ref('')
const statusFilter = ref('')
const projectFilter = ref('')
const page = ref(1)
const pageSize = 10
const selected = ref<string[]>([])
const bulkBusy = ref(false)

function aliasPreview(d: any) {
  const names: string[] = Array.isArray(d.hostnames) ? d.hostnames : [d.name]
  const extras = names.filter((n) => n && n !== d.name)
  if (!extras.length) return ''
  return extras.slice(0, 2).join('、') + (extras.length > 2 ? '…' : '')
}

function isBusy(id: string) {
  return busyIds.value.has(id)
}

function setBusy(id: string, on: boolean) {
  const next = new Set(busyIds.value)
  if (on) next.add(id)
  else next.delete(id)
  busyIds.value = next
}

function cdnTitle(d: any) {
  if (d.status !== 'active') return '请先启用站点'
  return d.cdn_enabled !== false ? 'CDN 加速中，点击关闭' : '仅 DNS，点击开启加速'
}

function cacheTitle(d: any) {
  if (d.status !== 'active') return '请先启用站点'
  return d.cache_enabled !== false ? '缓存已开启，点击关闭' : '缓存已关闭，点击开启'
}

const filtered = computed(() => {
  const q = keyword.value.trim().toLowerCase()
  return domains.value.filter((d) => {
    if (statusFilter.value && d.status !== statusFilter.value) return false
    if (projectFilter.value === 'none' && d.project_id) return false
    if (projectFilter.value && projectFilter.value !== 'none' && d.project_id !== projectFilter.value)
      return false
    if (!q) return true
    const hostnames: string[] = Array.isArray(d.hostnames) ? d.hostnames : [d.name]
    return (
      String(d.name).toLowerCase().includes(q)
      || String(d.display_name || '').toLowerCase().includes(q)
      || String(d.origin_host).toLowerCase().includes(q)
      || hostnames.some((h) => String(h).toLowerCase().includes(q))
    )
  })
})

const paged = computed(() => {
  const start = (page.value - 1) * pageSize
  return filtered.value.slice(start, start + pageSize)
})

const allPageSelected = computed(
  () => paged.value.length > 0 && paged.value.every((d) => selected.value.includes(d.id)),
)

function togglePage() {
  if (allPageSelected.value) {
    selected.value = selected.value.filter((id) => !paged.value.some((d) => d.id === id))
  } else {
    selected.value = [...new Set([...selected.value, ...paged.value.map((d) => d.id)])]
  }
}

async function bulkStatus(status: string) {
  if (!selected.value.length || bulkBusy.value) return
  const ids = [...selected.value]
  bulkBusy.value = true
  try {
    const results = await Promise.allSettled(
      ids.map((id) => api.post(`/api/v1/domains/${id}/status`, { status })),
    )
    const failed = results.filter((r) => r.status === 'rejected').length
    if (failed) toast.error(`${failed}/${ids.length} 个站点状态变更失败`)
    else toast.success(`已更新 ${ids.length} 个站点状态`)
    selected.value = []
    await load()
  } finally {
    bulkBusy.value = false
  }
}

async function bulkDelete() {
  if (!selected.value.length || bulkBusy.value) return
  const ok = await ask({
    title: '批量删除站点',
    message: `将删除 ${selected.value.length} 个站点及其规则与域名，此操作不可恢复。`,
    confirmText: '确认删除',
    danger: true,
  })
  if (!ok) return
  const ids = [...selected.value]
  bulkBusy.value = true
  try {
    const results = await Promise.allSettled(ids.map((id) => api.del(`/api/v1/domains/${id}`)))
    const failed = results.filter((r) => r.status === 'rejected').length
    if (failed) toast.error(`${failed}/${ids.length} 个站点删除失败`)
    else toast.success(`已删除 ${ids.length} 个站点`)
    selected.value = []
    await load()
  } finally {
    bulkBusy.value = false
  }
}

function exportDomains() {
  exportCsv(`lagran-sites-${new Date().toISOString().slice(0, 10)}`, filtered.value, [
    { key: 'display_name', label: '站点名称' },
    { key: 'name', label: '主域名' },
    { key: 'hostnames', label: '全部域名' },
    { key: 'status', label: '状态' },
    { key: 'cdn_enabled', label: 'CDN 加速' },
    { key: 'cache_enabled', label: '缓存' },
    { key: 'origin_proto', label: '源站协议' },
    { key: 'origin_host', label: '源站主机' },
    { key: 'origin_port', label: '源站端口' },
    { key: 'ssl_enabled', label: 'HTTPS' },
    { key: 'rate_limit_rps', label: '限流 QPS' },
    { key: 'created_at', label: '创建时间' },
  ])
}

watch([keyword, statusFilter, projectFilter], () => {
  page.value = 1
})

async function load() {
  loading.value = true
  try {
    let failed = 0
    const track = <T,>(p: Promise<T>, fallback: T) =>
      p.catch(() => {
        failed += 1
        return fallback
      })
    const [d, p] = await Promise.all([
      track(api.get<any[]>('/api/v1/domains'), []),
      track(api.get<any[]>('/api/v1/projects'), []),
    ])
    domains.value = d || []
    projects.value = p || []
    if (failed) toast.error(`有 ${failed} 项数据加载失败`)
  } finally {
    loading.value = false
  }
}

async function onProjectChange(d: any, value: string) {
  try {
    await api.put(`/api/v1/domains/${d.id}`, { project_id: value || null })
    toast.success('项目归属已更新')
    await load()
  } catch (e: any) {
    toast.error(e?.data?.error || '更新项目归属失败')
    await load()
  }
}

function patchDomain(id: string, patch: Record<string, any>) {
  const i = domains.value.findIndex((x) => x.id === id)
  if (i >= 0) domains.value[i] = { ...domains.value[i], ...patch }
}

async function toggleCdn(d: any) {
  if (isBusy(d.id) || d.status !== 'active') return
  const next = d.cdn_enabled === false
  const ok = await ask({
    title: next ? '开启 CDN 加速' : '关闭 CDN 加速',
    message: next
      ? `开启后 ${siteLabel(d)} 将重新走 CDN。确认继续？`
      : d.access_mode === 'ns'
        ? `关闭后 DNS 记录保留，访客直连源站。确认继续？`
        : `关闭后需在注册商把解析改回源站才能真正直回源。确认继续？`,
    confirmText: next ? '开启加速' : '关闭加速',
    danger: !next,
  })
  if (!ok) return
  setBusy(d.id, true)
  try {
    await api.post(`/api/v1/domains/${d.id}/cdn`, { cdn_enabled: next })
    patchDomain(d.id, { cdn_enabled: next })
    toast.success(next ? '已开启加速' : '已关闭加速')
  } catch (e: any) {
    toast.error(e?.data?.error || '切换失败')
  } finally {
    setBusy(d.id, false)
  }
}

async function toggleCache(d: any) {
  if (isBusy(d.id) || d.status !== 'active') return
  const next = d.cache_enabled === false
  const ok = await ask({
    title: next ? '开启站点缓存' : '关闭站点缓存',
    message: next
      ? `开启后 ${siteLabel(d)} 将按缓存规则读写缓存。确认继续？`
      : `关闭后所有请求不读写缓存，并立即清理该站点已有缓存。确认继续？`,
    confirmText: next ? '开启缓存' : '关闭并清理',
    danger: !next,
  })
  if (!ok) return
  setBusy(d.id, true)
  try {
    await api.post(`/api/v1/domains/${d.id}/cache`, { cache_enabled: next })
    patchDomain(d.id, { cache_enabled: next })
    toast.success(next ? '已开启缓存' : '已关闭缓存并下发清理')
  } catch (e: any) {
    toast.error(e?.data?.error || '切换失败')
  } finally {
    setBusy(d.id, false)
  }
}

async function toggleStatus(d: any) {
  if (isBusy(d.id)) return
  const next = d.status === 'disabled' ? 'active' : 'disabled'
  const ok = await ask({
    title: next === 'disabled' ? '停用站点' : '启用站点',
    message:
      next === 'disabled'
        ? `停用后，访问 ${d.name} 将返回「未接入」提示页，确认继续？`
        : `确认重新启用 ${siteLabel(d)}？`,
    confirmText: next === 'disabled' ? '停用' : '启用',
    danger: next === 'disabled',
  })
  if (!ok) return
  setBusy(d.id, true)
  try {
    await api.post(`/api/v1/domains/${d.id}/status`, { status: next })
    patchDomain(d.id, { status: next })
    toast.success(next === 'disabled' ? '站点已停用' : '站点已启用')
  } catch (e: any) {
    toast.error(e?.data?.error || '状态更新失败')
  } finally {
    setBusy(d.id, false)
  }
}

onMounted(load)
</script>

<style scoped>
.search-box {
  position: relative;
  display: flex;
  align-items: center;
}

.search-icon {
  position: absolute;
  left: 10px;
  color: var(--text-3);
  pointer-events: none;
}

.proj-select {
  height: 30px;
  min-width: 120px;
  font-size: 13px;
}

.switch-col {
  width: 64px;
  text-align: center;
  white-space: nowrap;
}

.ops-col {
  width: 100px;
  white-space: nowrap;
}

.origin-cell {
  font-size: 12px;
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-off .cell-main,
.row-off .origin-cell,
.row-off .cell-sub {
  color: var(--text-3);
}

.switch.disabled {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}
</style>
