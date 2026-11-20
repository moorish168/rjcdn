<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">边缘节点</div>
        <div class="page-desc">
          在列表里添加、安装、升级、启停节点。心跳约 15 秒一次，60 秒无心跳记为离线
        </div>
      </div>
      <div class="page-actions">
        <label class="switch" :class="{ on: autoRefresh }">
          <span class="switch-track" />
          <input v-model="autoRefresh" type="checkbox" style="display: none" />
          自动刷新
        </label>
        <button class="btn btn-default" type="button" @click="exportNodes">
          <AppIcon name="upload" :size="15" />
          导出 CSV
        </button>
        <button class="btn btn-default" type="button" :disabled="loading" @click="load">
          <AppIcon name="refresh" :size="15" />
          刷新
        </button>
        <button class="btn btn-default" type="button" @click="openSchedule">
          <AppIcon name="globe" :size="15" />
          调度测试
        </button>
        <button class="btn btn-default" type="button" @click="openDnsTest">
          <AppIcon name="search" :size="15" />
          DNS 测试
        </button>
        <NuxtLink to="/nodes/onboard" class="btn btn-primary">
          <AppIcon name="plus" :size="15" />
          添加节点
        </NuxtLink>
      </div>
    </div>

    <div class="stat-grid">
      <StatCard label="节点总数" :value="nodes.length" icon="node" />
      <StatCard label="在线节点" :value="counts.online" icon="activity" tone="green" />
      <StatCard label="待接入 / 离线" :value="counts.pending + counts.offline" hint="安装后等待首次心跳" icon="clock" tone="orange" />
      <StatCard label="已停用" :value="counts.disabled" icon="alert" tone="red" />
    </div>

    <div class="card">
      <div class="card-head">
        <div>
          <div class="card-title">节点列表</div>
          <div class="card-desc">停用后节点立即停止代理流量，但仍保留心跳与管理接口</div>
        </div>
        <div class="card-extra text-muted" style="font-size: 12px">
          {{ lastUpdated ? `更新于 ${lastUpdated}` : '' }}
        </div>
      </div>

      <div v-if="nodes.length" class="filter-bar">
        <input
          v-model="keyword"
          class="input"
          placeholder="搜索名称、IP、区域或线路"
          style="min-width: 240px"
        />
        <span class="spacer" />
        <span class="text-muted" style="font-size: 13px">{{ filteredNodes.length }} 台</span>
      </div>

      <div v-if="filteredNodes.length" class="table-wrap">
        <table class="table">
          <thead>
            <tr>
              <th style="min-width: 190px">节点</th>
              <th>区域 / 线路</th>
              <th>状态</th>
              <th class="num">版本</th>
              <th style="min-width: 150px">资源</th>
              <th style="min-width: 190px">节点地址</th>
              <th style="min-width: 140px">最后心跳</th>
              <th style="min-width: 230px">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="n in pagedNodes" :key="n.id">
              <td>
                <div class="cell-main">{{ n.name }}</div>
                <div class="cell-sub mono">{{ n.id.slice(0, 8) }}</div>
              </td>
              <td>
                <div>{{ n.region || '默认区域' }}</div>
                <div class="cell-sub">
                  {{ [n.country, n.province, n.city, n.isp].filter(Boolean).join(' · ') || '未设置线路' }}
                </div>
                <div class="cell-sub mono">{{ n.public_ip || '未设置公网 IP' }}</div>
              </td>
              <td>
                <StatusTag :tone="statusTone(n.status)" dot>
                  {{ statusLabel(n.status) }}
                </StatusTag>
              </td>
              <td class="num text-2">{{ n.version || '—' }}</td>
              <td>
                <div class="resource">
                  <span class="res-label">CPU</span>
                  <div class="progress res-bar">
                    <div class="progress-bar"
                      :style="{ width: `${n.cpu_usage ?? 0}%`, background: usageColor(n.cpu_usage) }" />
                  </div>
                  <span class="res-val">{{ n.cpu_usage != null ? `${n.cpu_usage.toFixed(0)}%` : '—' }}</span>
                </div>
                <div class="resource">
                  <span class="res-label">MEM</span>
                  <div class="progress res-bar">
                    <div class="progress-bar"
                      :style="{ width: `${n.memory_usage ?? 0}%`, background: usageColor(n.memory_usage) }" />
                  </div>
                  <span class="res-val">{{ n.memory_usage != null ? `${n.memory_usage.toFixed(0)}%` : '—' }}</span>
                </div>
              </td>
              <td>
                <span class="mono text-2">{{ n.agent_url || '—' }}</span>
              </td>
              <td>
                <div>{{ fromNow(n.last_heartbeat) }}</div>
                <div class="cell-sub">{{ formatDateTime(n.last_heartbeat) }}</div>
              </td>
              <td>
                <div class="ops">
                  <NuxtLink :to="`/nodes/${n.id}/detail`" class="btn btn-text btn-sm">详情</NuxtLink>
                  <NuxtLink :to="`/nodes/${n.id}/settings`" class="btn btn-text btn-sm">设置</NuxtLink>
                  <NuxtLink :to="`/nodes/${n.id}/install`" class="btn btn-text btn-sm">安装</NuxtLink>
                  <button class="btn btn-text btn-sm" type="button" :disabled="upgradingId === n.id"
                    @click="upgrade(n)">
                    {{ upgradingId === n.id ? '升级中…' : '升级' }}
                  </button>
                  <button class="btn btn-text btn-sm" type="button" @click="toggle(n)">
                    {{ n.status === 'disabled' ? '启用' : '停用' }}
                  </button>
                  <NuxtLink :to="`/nodes/${n.id}/delete`" class="btn btn-text btn-sm danger">删除</NuxtLink>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div v-if="filteredNodes.length > pageSize" class="table-footer">
        <AppPagination
          v-model:page="page"
          v-model:page-size="pageSize"
          :total="filteredNodes.length"
          :page-size="pageSize"
          :page-sizes="[10, 20, 50]"
        />
      </div>

      <div v-if="!nodes.length && loading" class="card-body">
        <div class="skeleton" style="height: 34px; width: 100%" />
      </div>

      <EmptyState
        v-else-if="nodes.length && !filteredNodes.length"
        icon="search"
        title="没有匹配的节点"
        text="试试其他关键词，或清空搜索。"
      />

      <EmptyState v-else-if="!nodes.length" icon="node" title="还没有边缘节点"
        text="先添加节点，再在节点详情里远程安装；也可以手动把 edge.env 拷到机器启动。">
        <NuxtLink to="/nodes/onboard" class="btn btn-primary btn-sm">
          添加节点
        </NuxtLink>
      </EmptyState>
    </div>

    
    <!-- 调度测试 -->
    <BaseModal v-model="schedOpen" title="调度测试" width="720px" hide-footer>
      <div class="form-col">
        <div class="form-grid">
          <div class="field span-2">
            <label class="field-label">域名（可选）</label>
            <input v-model="schedForm.domain" class="input" placeholder="cdn.example.com" />
          </div>
          <div class="field">
            <label class="field-label">客户端 IP</label>
            <input v-model="schedForm.ip" class="input" placeholder="例如 203.0.113.9" />
            <span class="field-hint">用于匹配 GEOIP_JSON_PATH 的 CIDR 表。</span>
          </div>
          <div class="field">
            <label class="field-label">省份覆盖</label>
            <input v-model="schedForm.province" class="input" placeholder="例如 广东" />
          </div>
          <div class="field">
            <label class="field-label">运营商覆盖</label>
            <input v-model="schedForm.isp" class="input" placeholder="电信 / 联通 / 移动" />
          </div>
          <div class="field">
            <label class="field-label">城市覆盖</label>
            <input v-model="schedForm.city" class="input" placeholder="可选" />
          </div>
        </div>

        <div class="flex-between">
          <span class="text-muted" style="font-size: 12px">
            仅在线且填写了公网 IP 的节点参与调度；省份/运营商覆盖优先于 IP 解析。
          </span>
          <button class="btn btn-primary" type="button" :disabled="schedLoading" @click="runSchedule">
            {{ schedLoading ? '测试中…' : '开始测试' }}
          </button>
        </div>

        <template v-if="schedResult">
          <div class="alert alert-info" style="margin-top: 4px">
            <AppIcon name="info" :size="16" class="icon" />
            <span>
              客户端：{{ [schedResult.client.country, schedResult.client.province, schedResult.client.city, schedResult.client.isp].filter(Boolean).join(' · ') || '未知位置' }}
              · TTL {{ schedResult.ttl }}s · {{ schedResult.note }}
            </span>
          </div>
          <div class="sched-pick">
            <div class="sched-card primary">
              <div class="sched-label">主节点</div>
              <div class="sched-name">{{ schedResult.primary?.name || '无可用节点' }}</div>
              <div class="mono">{{ schedResult.primary?.public_ip || '—' }}</div>
              <div class="cell-sub">
                {{ [schedResult.primary?.country, schedResult.primary?.province, schedResult.primary?.city, schedResult.primary?.isp].filter(Boolean).join(' · ') || '未设置线路' }}
              </div>
            </div>
            <div class="sched-card">
              <div class="sched-label">备用节点</div>
              <div class="sched-name">{{ schedResult.backup?.name || '无' }}</div>
              <div class="mono">{{ schedResult.backup?.public_ip || '—' }}</div>
              <div class="cell-sub">
                {{ [schedResult.backup?.country, schedResult.backup?.province, schedResult.backup?.city, schedResult.backup?.isp].filter(Boolean).join(' · ') || '—' }}
              </div>
            </div>
          </div>

          <div class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>候选节点</th>
                  <th>国家 / 省 / 市 / 运营商</th>
                  <th class="num">权重</th>
                  <th class="num">得分</th>
                  <th>原因</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="c in schedResult.candidates" :key="c.name">
                  <td class="mono">{{ c.name }}</td>
                  <td>{{ [c.country, c.province, c.city, c.isp].filter(Boolean).join(' · ') || '—' }}</td>
                  <td class="num">{{ c.weight }}</td>
                  <td class="num">{{ c.score }}</td>
                  <td class="text-2" style="font-size: 12px">{{ c.reason }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div class="flex-between">
            <span />
            <button class="btn btn-default" type="button" @click="schedOpen = false">关闭</button>
          </div>
        </template>
      </div>
    </BaseModal>
    <!-- DNS 测试 -->
    <BaseModal v-model="dnsOpen" title="DNS 解析测试" width="620px" hide-footer>
      <div class="form-col">
        <div class="form-grid">
          <div class="field span-2">
            <label class="field-label">查询名称</label>
            <input v-model="dnsForm.name" class="input" placeholder="cname.lagranedge.com" />
          </div>
          <div class="field span-2">
            <label class="field-label">客户端 IP / 子网（可选，作为 ECS）</label>
            <input v-model="dnsForm.ip" class="input" placeholder="203.0.113.9/24" />
            <span class="field-hint">向平台智能 DNS 发起真实解析查询，用于确认解析是否正常。</span>
          </div>
        </div>
        <div class="flex-between">
          <span class="text-muted" style="font-size: 12px">
            返回 rcode / IP / TTL；DNS 未配置或超时会显示错误。
          </span>
          <button class="btn btn-primary" type="button" :disabled="dnsLoading" @click="runDnsTest">
            {{ dnsLoading ? '查询中…' : '查询' }}
          </button>
        </div>

        <template v-if="dnsResult">
          <div class="alert alert-info">
            <AppIcon name="info" :size="16" class="icon" />
            <span>
              {{ dnsResult.server }} · rcode={{ dnsResult.rcode }} ·
              {{ dnsResult.elapsed_ms }}ms{{ dnsResult.truncated ? ' · TC' : '' }}
            </span>
          </div>
          <div v-if="dnsResult.answers.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>IP</th>
                  <th class="num">TTL</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="a in dnsResult.answers" :key="a.ip">
                  <td class="mono">{{ a.ip }}</td>
                  <td class="num">{{ a.ttl }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else class="text-muted" style="font-size: 13px">
            没有 A 记录（rcode={{ dnsResult.rcode }}）。
          </div>
          <div class="flex-between">
            <span />
            <button class="btn btn-default" type="button" @click="dnsOpen = false">关闭</button>
          </div>
        </template>
      </div>
    </BaseModal>
  </div>
</template>

<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'

const api = useApi()
const toast = useToast()
const { ask } = useConfirm()

const nodes = ref<any[]>([])
const loading = ref(false)
const upgradingId = ref('')
const autoRefresh = ref(true)
const lastUpdated = ref('')

const schedOpen = ref(false)
const schedLoading = ref(false)
const schedResult = ref<any>(null)
const schedForm = ref({ domain: '', ip: '', province: '', isp: '', city: '' })

const dnsOpen = ref(false)
const dnsLoading = ref(false)
const dnsResult = ref<any>(null)
const dnsForm = ref({ name: 'cname.lagranedge.com', ip: '' })

let timer: ReturnType<typeof setInterval> | null = null

const page = ref(1)
const pageSize = ref(10)
const keyword = ref('')
const filteredNodes = computed(() => {
  const q = keyword.value.trim().toLowerCase()
  if (!q) return nodes.value
  return nodes.value.filter((n) =>
    [n.name, n.region, n.public_ip, n.agent_url, n.country, n.province, n.city, n.isp, n.id]
      .map((v) => String(v || '').toLowerCase())
      .some((v) => v.includes(q)),
  )
})
const pagedNodes = computed(() => {
  const start = (page.value - 1) * pageSize.value
  return filteredNodes.value.slice(start, start + pageSize.value)
})

function exportNodes() {
  exportCsv(`lagran-nodes-${new Date().toISOString().slice(0, 10)}`, nodes.value, [
    { key: 'name', label: '节点' },
    { key: 'region', label: '区域' },
    { key: 'status', label: '状态' },
    { key: 'version', label: '版本' },
    { key: 'public_ip', label: '公网 IP' },
    { key: 'agent_url', label: '节点地址' },
    { key: 'cpu_usage', label: 'CPU%' },
    { key: 'memory_usage', label: '内存%' },
    { key: 'last_heartbeat', label: '最后心跳' },
  ])
}

watch(
  () => [filteredNodes.value.length, pageSize.value, keyword.value] as const,
  ([total]) => {
    const last = Math.max(1, Math.ceil(Number(total) / pageSize.value))
    if (page.value > last) page.value = last
  },
)

const counts = computed(() => {
  const result = { online: 0, offline: 0, pending: 0, disabled: 0 }
  for (const n of nodes.value) {
    if (n.status === 'online') result.online += 1
    else if (n.status === 'pending') result.pending += 1
    else if (n.status === 'disabled') result.disabled += 1
    else result.offline += 1
  }
  return result
})

function statusLabel(status: string) {
  return (
    {
      online: '在线',
      offline: '离线',
      pending: '待接入',
      disabled: '已停用',
    } as Record<string, string>
  )[status] || status
}

function statusTone(status: string) {
  if (status === 'online') return 'success'
  if (status === 'pending') return 'warning'
  if (status === 'disabled') return 'danger'
  return 'neutral'
}

function usageColor(value?: number | null) {
  if (value == null) return 'var(--brand)'
  if (value >= 85) return 'var(--danger)'
  if (value >= 70) return 'var(--warning)'
  return 'var(--brand)'
}

async function load() {
  loading.value = true
  try {
    nodes.value = (await api.get<any[]>('/api/v1/nodes')) || []
    lastUpdated.value = new Date().toLocaleTimeString('zh-CN', { hour12: false })
  } catch (e: any) {
    // keep previous list
    toast.error(e?.data?.error || e?.message || '加载节点失败')
  } finally {
    loading.value = false
  }
}

function openSchedule() {
  schedResult.value = null
  schedForm.value = { domain: '', ip: '', province: '', isp: '', city: '' }
  schedOpen.value = true
}

async function runSchedule() {
  schedLoading.value = true
  try {
    schedResult.value = await api.post('/api/v1/schedule/test', {
      domain: schedForm.value.domain.trim(),
      ip: schedForm.value.ip.trim(),
      province: schedForm.value.province.trim(),
      isp: schedForm.value.isp.trim(),
      city: schedForm.value.city.trim(),
    })
  } catch (e: any) {
    toast.error(e?.data?.error || '调度测试失败')
  } finally {
    schedLoading.value = false
  }
}

function openDnsTest() {
  dnsResult.value = null
  dnsOpen.value = true
}

async function runDnsTest() {
  if (!dnsForm.value.name.trim()) {
    toast.error('请填写查询名称')
    return
  }
  dnsLoading.value = true
  try {
    dnsResult.value = await api.post('/api/v1/dns/test', {
      name: dnsForm.value.name.trim(),
      ip: dnsForm.value.ip.trim(),
    })
  } catch (e: any) {
    toast.error(e?.data?.error || 'DNS 查询失败')
  } finally {
    dnsLoading.value = false
  }
}

async function upgrade(node: any) {
  if (!node.agent_url || !node.has_token) {
    toast.error('该节点缺少节点地址或令牌，无法远程升级')
    return
  }
  const ok = await ask({
    title: '升级边缘节点',
    message: `将让 ${node.name} 从控制面获取最新 Release，下载校验后自动替换二进制并重启。升级期间该节点流量会短暂中断。确认继续？`,
    confirmText: '开始升级',
  })
  if (!ok) return
  upgradingId.value = node.id
  let queued = false
  try {
    const result = await api.post<any>(`/api/v1/nodes/${node.id}/upgrade`, {})
    queued = true
    toast.success(`升级任务已下发：${result.version}，节点将在下载校验后自动重启`)
    window.setTimeout(load, 5000)
    window.setTimeout(load, 15000)
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '升级任务下发失败'))
  } finally {
    if (!queued) {
      upgradingId.value = ''
    } else {
      window.setTimeout(() => {
        if (upgradingId.value === node.id) upgradingId.value = ''
      }, 20000)
    }
  }
}

async function toggle(node: any) {
  const enable = node.status === 'disabled'
  if (!enable) {
    const ok = await ask({
      title: '停用节点',
      message: `停用后 ${node.name} 将立即返回 503，不再代理任何流量；心跳和健康检查仍然保留。确认停用？`,
      confirmText: '停用',
      danger: true,
    })
    if (!ok) return
  }
  try {
    await api.post(`/api/v1/nodes/${node.id}/status`, { enabled: enable })
    toast.success(enable ? '节点已启用，等待下一次心跳' : '节点已停用，边缘将在下一次心跳后停止代理')
    await load()
  } catch (e: any) {
    toast.error(e?.data?.error || '状态更新失败')
  }
}

watch(autoRefresh, (on) => {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
  if (on) timer = setInterval(load, 10000)
})

onMounted(() => {
  load()
  if (autoRefresh.value) timer = setInterval(load, 10000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<style scoped>
.resource {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 2px 0;
}

.res-label {
  width: 30px;
  font-size: 11px;
  color: var(--text-3);
}

.res-bar {
  flex: 1;
  min-width: 60px;
}

.res-val {
  width: 30px;
  text-align: right;
  font-size: 11px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}

.sched-pick {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.sched-card {
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: #fafbfc;
}

.sched-card.primary {
  border-color: #bedaff;
  background: var(--brand-bg);
}

.sched-label {
  font-size: 11.5px;
  color: var(--text-3);
}

.sched-name {
  margin-top: 4px;
  font-size: 14px;
  font-weight: 500;
  color: var(--text);
}

@media (max-width: 640px) {
  .table {
    min-width: 900px;
  }
}
</style>
