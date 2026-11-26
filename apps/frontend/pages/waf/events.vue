<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">WAF 事件</div>
        <div class="page-desc">查看各站点的 WAF 命中记录；全局黑名单会作用于所有站点，包括 WAF 开关关闭的站点。</div>
      </div>
      <button class="btn btn-default" type="button" :disabled="loading" @click="load()">
        <AppIcon name="refresh" :size="15" />刷新
      </button>
    </div>

    <div class="card">
      <div class="filter-bar">
        <select v-model="domain" class="select domain-filter">
          <option value="">全部站点</option>
          <option v-for="item in domains" :key="item.id" :value="item.name">{{ item.name }}</option>
        </select>
        <select v-model="action" class="select">
          <option value="">全部动作</option>
          <option value="block">拦截</option>
          <option value="challenge">安全验证</option>
          <option value="monitor">观察</option>
          <option value="log">仅记录</option>
        </select>
        <select v-model.number="pageSize" class="select">
          <option :value="50">50 条 / 页</option>
          <option :value="100">100 条 / 页</option>
          <option :value="200">200 条 / 页</option>
          <option :value="500">500 条 / 页（接口上限）</option>
        </select>
        <button class="btn btn-default" type="button" :disabled="loading" @click="load(1)">查询</button>
        <span class="spacer" />
        <span class="text-muted">第 {{ page }} 页 · 本页 {{ events.length }} 条</span>
      </div>

      <div v-if="events.length" class="table-wrap">
        <table class="table event-table">
          <thead>
            <tr>
              <th>时间</th>
              <th>站点</th>
              <th>动作</th>
              <th>类型</th>
              <th>规则</th>
              <th>客户端 IP</th>
              <th>方法</th>
              <th>路径</th>
              <th>节点</th>
              <th v-if="canBanEvents">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="event in events" :key="event.id">
              <td class="text-muted nowrap">{{ formatDateTime(event.ts) }}</td>
              <td class="text-2">{{ event.domain }}</td>
              <td><StatusTag :tone="actionTone(event.action)">{{ actionLabel(event.action) }}</StatusTag></td>
              <td>{{ categoryLabel(event.category) }}</td>
              <td class="text-2">{{ event.rule_name || '—' }}</td>
              <td class="mono ip-cell" :title="event.client_ip">{{ event.client_ip || '—' }}</td>
              <td class="mono">{{ event.method }}</td>
              <td class="mono path-cell" :title="event.path">{{ event.path }}</td>
              <td class="text-2">{{ event.node_id }}</td>
              <td v-if="canBanEvents">
                <button
                  class="btn btn-text btn-sm danger"
                  type="button"
                  :disabled="banBusy !== null || !event.client_ip"
                  @click="quickBan(event)"
                >
                  {{ banBusy === event.id ? '处理中…' : auth.isAdmin ? '拉黑全局' : '拉黑本站' }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <div v-else-if="loading" class="card-body">
        <div class="skeleton" style="height: 34px; width: 100%" />
      </div>
      <EmptyState v-else icon="activity" title="暂无 WAF 事件" text="符合筛选条件的事件会显示在这里。" />

      <div v-if="events.length || page > 1" class="table-footer event-pagination">
        <span class="text-muted">第 {{ page }} 页 · {{ hasMore ? '还有后续记录' : '已到末页' }}</span>
        <div class="page-actions">
          <button class="btn btn-default btn-sm" type="button" :disabled="page <= 1 || loading" @click="load(page - 1)">上一页</button>
          <button class="btn btn-default btn-sm" type="button" :disabled="!hasMore || loading" @click="load(page + 1)">下一页</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
const api = useApi()
const toast = useToast()
const auth = useAuthStore()
const route = useRoute()

const domains = ref<any[]>([])
const events = ref<any[]>([])
const loading = ref(false)
const initialDomain = String(route.query.domain || '')
const domain = ref('')
const action = ref('')
const page = ref(1)
const pageSize = ref(100)
const hasMore = ref(false)
const banBusy = ref<string | number | null>(null)
const canBanEvents = computed(() => auth.isAdmin || auth.can('waf:write'))

function actionLabel(value: string) {
  return ({ block: '拦截', challenge: '安全验证', monitor: '观察', log: '仅记录' } as Record<string, string>)[value] || value
}

function actionTone(value: string) {
  if (value === 'block') return 'danger'
  if (value === 'challenge') return 'warning'
  return 'info'
}

function categoryLabel(value: string) {
  return ({
    ip_deny: 'IP 黑名单',
    global_ip_deny: '全局 IP 黑名单',
    ip_allow: 'IP 白名单',
    sqli: 'SQL 注入',
    xss: 'XSS',
    traversal: '路径穿越',
    scanners: '扫描器',
    rce: '命令执行',
    ssrf: 'SSRF',
    xxe: 'XXE',
    protocol: '协议异常',
    custom: '自定义规则',
  } as Record<string, string>)[value] || value || '—'
}

async function load(nextPage = page.value) {
  loading.value = true
  try {
    const q = new URLSearchParams({ page: String(nextPage), page_size: String(pageSize.value) })
    if (domain.value) q.set('domain', domain.value)
    if (action.value) q.set('action', action.value)
    const result = await api.get<any>(`/api/v1/waf/events?${q.toString()}`)
    events.value = result?.items || []
    page.value = result?.page || nextPage
    hasMore.value = !!result?.has_more
  } catch (e: any) {
    events.value = []
    hasMore.value = false
    toast.error(e?.data?.error || e?.message || '加载 WAF 事件失败')
  } finally {
    loading.value = false
  }
}

async function quickBan(event: any) {
  const ip = String(event?.client_ip || '').trim()
  if (!ip || banBusy.value !== null) return
  banBusy.value = event.id
  try {
    const payload = {
      ip,
      note: `WAF event ${event.category || ''} ${event.rule_name || ''}`.trim(),
    }
    if (auth.isAdmin) {
      await api.post('/api/v1/waf/global-ips/ban', payload)
      toast.success(`${ip} 已加入全局黑名单，将应用到所有站点`)
    } else {
      const site = domains.value.find((item) => item.name === event.domain)
      if (!site) throw new Error('找不到对应站点，无法添加本站黑名单')
      await api.post(`/api/v1/domains/${site.id}/waf/ips`, {
        kind: 'deny',
        ip,
        note: payload.note,
        enabled: true,
      })
      toast.success(`${ip} 已加入本站 WAF 黑名单`)
    }
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '拉黑失败')
  } finally {
    banBusy.value = null
  }
}

watch([domain, action, pageSize], () => load(1))

onMounted(async () => {
  domains.value = (await api.get<any[]>('/api/v1/domains').catch(() => [])) || []
  if (initialDomain) {
    domain.value = domains.value.find((item) => item.id === initialDomain || item.name === initialDomain)?.name || ''
  }
  await load(1)
})
</script>

<style scoped>
.domain-filter {
  min-width: 220px;
}

.event-table {
  min-width: 1350px;
}

.ip-cell {
  min-width: 160px;
  overflow-wrap: anywhere;
  white-space: normal;
}

.path-cell {
  max-width: 320px;
  overflow-wrap: anywhere;
  white-space: normal;
}

.event-pagination {
  justify-content: space-between;
}
</style>
