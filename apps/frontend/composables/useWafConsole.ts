import { friendlyErrorMessage } from '~/utils/friendlyError'

export type WafTab = 'overview' | 'settings' | 'rules' | 'ips' | 'geo' | 'bans' | 'events'

const WAF_TABS: WafTab[] = ['overview', 'settings', 'rules', 'ips', 'geo', 'bans', 'events']

export function useWafConsole(opts?: { domainId?: Ref<string> | ComputedRef<string> }) {
const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const auth = useAuthStore()
const route = useRoute()

type Tab = WafTab

const domains = ref<any[]>([])
/** Bound from site route when opened under /domains/:id/waf; otherwise query-driven. */
const domainId = ref(unref(opts?.domainId) || '')
if (opts?.domainId) {
  watch(
    () => unref(opts.domainId!),
    (id) => {
      const next = String(id || '').trim()
      if (next && domainId.value !== next) domainId.value = next
    },
    { immediate: true },
  )
}
const section = computed<Tab>(() => {
  // Prefer last path segment (/domains/:id/waf/rules → rules).
  const parts = route.path.split('/').filter(Boolean)
  const raw = String(route.params.section || parts[parts.length - 1] || 'overview')
  return (WAF_TABS.includes(raw as Tab) ? raw : 'overview') as Tab
})
const loading = ref(false)
const saving = ref(false)

const stats = ref<any>({
  days: 7,
  total: 0,
  by_action: [],
  by_category: [],
  by_day: [],
  top_ips: [],
  top_rules: [],
  top_paths: [],
})
const statsLoading = ref(false)

const securityLevels = [
  { value: 'off', label: '关闭' },
  { value: 'low', label: '低' },
  { value: 'medium', label: '中' },
  { value: 'high', label: '高' },
  { value: 'under_attack', label: '攻击中' },
]

const levelHint = computed(() => {
  const hints: Record<string, string> = {
    off: '只执行 IP 名单与显式规则，不按安全级别额外要求验证。',
    low: '对空浏览器标识的浏览请求发起安全验证。',
    medium: '在低级别基础上，对常见扫描工具与空浏览器标识发起安全验证。',
    high: '对所有网页浏览请求发起安全验证，静态资源除外。',
    under_attack: '对所有浏览请求发起安全验证，静态资源除外；适合正在被攻击时临时开启。',
  }
  return hints[settings.value.security_level] || hints.off
})

const settings = ref<any>({
  enabled: false,
  mode: 'block',
  sqli: true,
  xss: true,
  traversal: true,
  scanners: true,
  rce: true,
  ssrf: true,
  xxe: true,
  log4shell: true,
  crlf: true,
  ssti: true,
  nosql: true,
  ldap: true,
  java: true,
  webshell: true,
  protocol: true,
  security_level: 'off',
  challenge_difficulty: 4,
  challenge_ttl_secs: 1800,
  managed_rulesets: [] as string[],
  waf_rulepack_version: '',
  waf_gray_percent: 0,
})

const rules = ref<any[]>([])
const rulesets = ref<any[]>([])
const rulepackVersions = ref<any[]>([])
const rulepackBusy = ref(false)
const ips = ref<any[]>([])
const ipBindings = ref<any[]>([])
const availableIpLists = ref<any[]>([])
const bindListOpen = ref(false)
const bindListId = ref('')
const bindSaving = ref(false)
const geoRules = ref<any[]>([])
const geoPage = ref(1)
const geoPageSize = 10
const pagedGeo = computed(() => {
  const start = (geoPage.value - 1) * geoPageSize
  return geoRules.value.slice(start, start + geoPageSize)
})
const rulePage = ref(1)
const ipPage = ref(1)
const rulePageSize = 10
const ipPageSize = 10
const pagedRules = computed(() => {
  const start = (rulePage.value - 1) * rulePageSize
  return rules.value.slice(start, start + rulePageSize)
})
const pagedIps = computed(() => {
  const start = (ipPage.value - 1) * ipPageSize
  return ips.value.slice(start, start + ipPageSize)
})
watch(
  () => rules.value.length,
  (total) => {
    rulePage.value = Math.min(rulePage.value, Math.max(1, Math.ceil(total / rulePageSize)))
  },
)
watch(
  () => ips.value.length,
  (total) => {
    ipPage.value = Math.min(ipPage.value, Math.max(1, Math.ceil(total / ipPageSize)))
  },
)
watch(
  () => geoRules.value.length,
  (total) => {
    geoPage.value = Math.min(geoPage.value, Math.max(1, Math.ceil(total / geoPageSize)))
  },
)
const events = ref<any[]>([])
const eventsLoading = ref(false)
const eventAction = ref('')
const eventLimit = ref(100)

const bans = ref<any[]>([])
const bansLoading = ref(false)
const bansError = ref('')
const banStatusFilter = ref('')
const currentDomainName = computed(
  () => domains.value.find((d) => d.id === domainId.value)?.name || ''
)
const filteredBans = computed(() => {
  if (!banStatusFilter.value) return bans.value
  return bans.value.filter((b) => b.status === banStatusFilter.value)
})
const ccStats = ref<any>(null)

const ruleModalOpen = ref(false)
const editingRule = ref<any>(null)
const ruleError = ref('')
const ruleForm = ref({
  name: '',
  field: 'path',
  header_name: '',
  operator: 'contains',
  value: '',
  action: 'block',
  priority: 0,
  enabled: true,
  expr: '',
})

const ipModalOpen = ref(false)
const ipError = ref('')
const editingIp = ref<any>(null)
const ipForm = ref({ kind: 'deny', ip: '', note: '', enabled: true })
const geoModalOpen = ref(false)
const geoError = ref('')
const editingGeo = ref<any>(null)
/** Continent → country catalog, shared with the DNS line picker. */
const geo = useGeoCatalog()
const geoForm = ref({
  action: 'deny' as 'allow' | 'deny',
  /** Selected ISO country codes (whole continents expanded). */
  countries: [] as string[],
  note: '',
  enabled: true,
})

const banModalOpen = ref(false)
const banError = ref('')
const banForm = ref({ domain: '', ip: '', seconds: 3600, reason: 'manual-cc-ban' })

const managedRules = [
  { key: 'sqli', label: 'SQL 注入', desc: 'union select、or 1=1、sleep() 等特征' },
  { key: 'xss', label: 'XSS 跨站脚本', desc: '\u003cscript\u003e、javascript:、onerror= 等' },
  { key: 'traversal', label: '路径穿越', desc: '路径跳转 ../、/etc/passwd 读取' },
  { key: 'scanners', label: '扫描器与敏感路径', desc: 'sqlmap/nikto UA、/.git/config 等' },
  { key: 'rce', label: '命令执行 / RCE', desc: ';wget、|bash、powershell、$() 等' },
  { key: 'ssrf', label: 'SSRF / 云元数据', desc: '169.254.169.254、metadata、gopher:// 等' },
  { key: 'xxe', label: 'XXE 外部实体', desc: '<!ENTITY、system "、file:///etc 等' },
  { key: 'log4shell', label: 'Log4Shell', desc: '${jndi:ldap://}、jndi:rmi、大小写混淆等' },
  { key: 'crlf', label: 'CRLF / 头注入', desc: '路径或查询串中的 \\r\\n、%0d%0a' },
  { key: 'ssti', label: '模板注入 SSTI', desc: '{{7*7}}、${7*7}、__class__、FreeMarker 等' },
  { key: 'nosql', label: 'NoSQL 注入', desc: '$ne / $gt / $where / $regex 等操作符注入' },
  { key: 'ldap', label: 'LDAP 注入', desc: '*)(uid=*、)(|(、objectClass=* 等' },
  { key: 'java', label: 'Java 反序列化 / OGNL', desc: 'ProcessBuilder、getRuntime、OGNL、Spring4Shell 等' },
  { key: 'webshell', label: 'WebShell / PHP 注入', desc: '<?php、php://filter、eval()、base64_decode() 等' },
  { key: 'protocol', label: '协议异常', desc: '空字节、双重编码、超长路径或查询串' },
]

const tabs = computed(() => [
  { value: 'overview' as Tab, label: '数据概览' },
  { value: 'settings' as Tab, label: '防护设置' },
  { value: 'rules' as Tab, label: '自定义规则', count: rules.value.length },
  { value: 'ips' as Tab, label: 'IP 黑白名单', count: ips.value.length + ipBindings.value.length },
  { value: 'geo' as Tab, label: '地区名单', count: geoRules.value.length },
  { value: 'bans' as Tab, label: '共享封禁', count: bans.value.length },
  { value: 'events' as Tab, label: '事件日志' },
])

function actionCount(action: string) {
  const row = (stats.value.by_action || []).find((r: any) => r.key === action)
  return row ? row.count : 0
}

const maxDayCount = computed(() => {
  const rows = stats.value.by_day || []
  return rows.reduce((m: number, r: any) => Math.max(m, r.count), 0) || 1
})

function barHeight(count: number) {
  return `${Math.max(4, Math.round((count / maxDayCount.value) * 100))}%`
}

async function loadStats() {
  if (!domainId.value) return
  statsLoading.value = true
  try {
    const name = domains.value.find((d) => d.id === domainId.value)?.name || ''
    const q = new URLSearchParams({ days: '7' })
    if (name) q.set('domain', name)
    stats.value = await api.get<any>(`/api/v1/waf/stats?${q.toString()}`)
    statsLoading.value = false
  } catch {
    stats.value = {
      days: 7,
      total: 0,
      by_action: [],
      by_category: [],
      by_day: [],
      top_ips: [],
      top_rules: [],
      top_paths: [],
    }
    statsLoading.value = false
  }
}

const filteredEvents = computed(() => {
  if (!eventAction.value) return events.value
  return events.value.filter((e) => e.action === eventAction.value)
})

const eventPage = ref(1)
const eventPageSize = ref(20)
const pagedEvents = computed(() => {
  const start = (eventPage.value - 1) * eventPageSize.value
  return filteredEvents.value.slice(start, start + eventPageSize.value)
})
watch(eventAction, () => {
  eventPage.value = 1
})
watch(eventLimit, () => {
  eventPage.value = 1
})
watch(
  () => filteredEvents.value.length,
  (total) => {
    eventPage.value = Math.min(
      eventPage.value,
      Math.max(1, Math.ceil(total / eventPageSize.value)),
    )
  },
)

function fieldLabel(field: string) {
  if (field && field.startsWith('header:')) return `请求头 ${field.slice(7)}`
  return (
    {
      path: '路径',
      query: '查询串',
      user_agent: 'UA',
      referer: 'Referer',
      method: '方法',
      ip: 'IP',
    } as Record<string, string>
  )[field] || field
}

function opLabel(op: string) {
  return ({ contains: '包含', equals: '等于', prefix: '前缀', suffix: '后缀' } as Record<string, string>)[op] || op
}

function actionLabel(action: string) {
  return (
    { block: '拦截', allow: '放行', log: '仅记录', challenge: '安全验证' } as Record<string, string>
  )[action] || action
}

function actionTone(action: string) {
  if (action === 'block') return 'danger'
  if (action === 'allow') return 'success'
  return 'info'
}

function eventLabel(action: string) {
  return (
    { block: '拦截', monitor: '仅记录', log: '记录', challenge: '安全验证' } as Record<string, string>
  )[action] || action
}

function eventTone(action: string) {
  return action === 'block' ? 'danger' : action === 'monitor' ? 'warning' : 'info'
}

function categoryLabel(category: string) {
  return (
    {
      sqli: 'SQL 注入',
      xss: 'XSS',
      traversal: '路径穿越',
      scanners: '扫描器',
      rce: '命令执行',
      ssrf: 'SSRF',
      xxe: 'XXE',
      log4shell: 'Log4Shell',
      crlf: 'CRLF 注入',
      ip_deny: 'IP 黑名单',
      geo_deny: '地区黑名单',
      geo_allow: '地区白名单未命中',
      // Older events, from rule types the console no longer offers.
      region_deny: '地区黑名单',
      asn_deny: '网络号黑名单',
      custom: '自定义规则',
      security_level: '安全级别',
    } as Record<string, string>
  )[category] || category || '—'
}

async function loadDomains() {
  domains.value = (await api.get<any[]>('/api/v1/domains').catch(() => [])) || []
  // Site-scoped console already has domainId from the route.
  if (opts?.domainId) {
    const fixed = String(unref(opts.domainId) || '').trim()
    if (fixed) domainId.value = fixed
    return
  }
  const fromParam = String(route.params.id || '').trim()
  if (fromParam && route.path.includes('/waf') && domains.value.some((d) => d.id === fromParam)) {
    domainId.value = fromParam
    return
  }
  const q = String(route.query.domain || '').trim()
  if (q && domains.value.some((d) => d.id === q)) {
    domainId.value = q
  } else if (!domainId.value && domains.value.length) {
    domainId.value = domains.value[0].id
  }
}

async function loadSettings() {
  if (!domainId.value) return
  try {
    const s = await api.get<any>(`/api/v1/domains/${domainId.value}/waf`)
    settings.value = {
      enabled: !!s.enabled,
      mode: s.mode === 'monitor' ? 'monitor' : 'block',
      sqli: !!s.sqli,
      xss: !!s.xss,
      traversal: !!s.traversal,
      scanners: !!s.scanners,
      rce: !!s.rce,
      ssrf: !!s.ssrf,
      xxe: !!s.xxe,
      log4shell: !!s.log4shell,
      crlf: !!s.crlf,
      ssti: !!s.ssti,
      nosql: !!s.nosql,
      ldap: !!s.ldap,
      java: !!s.java,
      webshell: !!s.webshell,
      protocol: !!s.protocol,
      security_level: s.security_level || 'off',
      challenge_difficulty: s.challenge_difficulty || 4,
      challenge_ttl_secs: s.challenge_ttl_secs || 1800,
      managed_rulesets: Array.isArray(s.managed_rulesets) ? s.managed_rulesets : [],
      waf_rulepack_version: s.waf_rulepack_version || '',
      waf_gray_percent: s.waf_gray_percent ?? 0,
    }
  } catch {
    settings.value = {
      enabled: false,
      mode: 'block',
      sqli: true,
      xss: true,
      traversal: true,
      scanners: true,
      rce: true,
      ssrf: true,
      xxe: true,
      log4shell: true,
      crlf: true,
      security_level: 'off',
      challenge_difficulty: 4,
      challenge_ttl_secs: 1800,
      managed_rulesets: [],
      waf_rulepack_version: '',
      waf_gray_percent: 0,
    }
  }
}

async function loadRulesets() {
  rulesets.value = (await api.get<any[]>('/api/v1/waf/rulesets').catch(() => [])) || []
}

function slotLabel(slot: string) {
  return ({ active: '当前版本', previous: '上一版本', retired: '已退役' } as Record<string, string>)[slot] || slot
}

function formatRulepackTime(value: string) {
  if (!value) return ''
  return value.slice(0, 16).replace('T', ' ')
}

async function loadRulepacks() {
  rulepackVersions.value = (await api.get<any[]>('/api/v1/waf/rulepacks').catch(() => [])) || []
}

async function publishRulepack() {
  rulepackBusy.value = true
  try {
    await api.post('/api/v1/waf/rulepacks/publish', {})
    toast.success('新规则包已发布并签名')
    await loadRulepacks()
  } catch (e: any) {
    toast.error(e?.data?.error || '发布失败')
  } finally {
    rulepackBusy.value = false
  }
}

async function retireRulepack(version: string) {
  const ok = await ask({
    title: '转为 retired',
    message: `将规则包 ${version} 转为 retired？转为 retired 后才能删除。`,
    confirmText: '转为 retired',
  })
  if (!ok) return
  rulepackBusy.value = true
  try {
    await api.post(`/api/v1/waf/rulepacks/${version}/retire`, {})
    toast.success(`规则包 ${version} 已转为 retired`)
    await loadRulepacks()
  } catch (e: any) {
    toast.error(e?.data?.error || '操作失败')
  } finally {
    rulepackBusy.value = false
  }
}

async function deleteRulepack(version: string) {
  const ok = await ask({
    title: '删除规则包',
    message: `删除规则包 ${version}？此操作不可撤销；仍有域名固定该版本时会被拒绝。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  rulepackBusy.value = true
  try {
    await api.del(`/api/v1/waf/rulepacks/${version}`)
    toast.success(`规则包 ${version} 已删除`)
    await loadRulepacks()
  } catch (e: any) {
    toast.error(e?.data?.error || '删除失败')
  } finally {
    rulepackBusy.value = false
  }
}

async function rollbackRulepack(version: string) {
  rulepackBusy.value = true
  try {
    await api.post(`/api/v1/waf/rulepacks/${version}/rollback`, {})
    toast.success(`规则包已切换到 ${version}`)
    await loadRulepacks()
  } catch (e: any) {
    toast.error(e?.data?.error || '回滚失败')
  } finally {
    rulepackBusy.value = false
  }
}

function toggleRuleset(id: string) {
  const selected = settings.value.managed_rulesets || []
  settings.value.managed_rulesets = selected.includes(id)
    ? selected.filter((item: string) => item !== id)
    : [...selected, id]
}

async function loadRules() {
  if (!domainId.value) return
  try {
    rules.value = (await api.get<any[]>(`/api/v1/domains/${domainId.value}/waf/rules`)) || []
  } catch (e: any) {
    rules.value = []
    toast.error(e?.data?.error || e?.message || '加载 WAF 规则失败')
  }
}

async function loadIps() {
  if (!domainId.value) return
  try {
    const [rows, bindings] = await Promise.all([
      api.get<any[]>(`/api/v1/domains/${domainId.value}/waf/ips`),
      api.get<any[]>(`/api/v1/domains/${domainId.value}/waf/ip-lists`),
    ])
    ips.value = rows || []
    ipBindings.value = bindings || []
  } catch (e: any) {
    ips.value = []
    ipBindings.value = []
    toast.error(e?.data?.error || e?.message || '加载 IP 名单失败')
  }
}

async function openBindListModal() {
  availableIpLists.value = (await api.get<any[]>('/api/v1/ip-lists').catch(() => [])) || []
  const bound = new Set(ipBindings.value.map((b) => b.list_id))
  availableIpLists.value = availableIpLists.value.filter((l) => !bound.has(l.id))
  bindListId.value = availableIpLists.value[0]?.id || ''
  bindListOpen.value = true
}

async function confirmBindList() {
  if (!domainId.value || !bindListId.value) return
  bindSaving.value = true
  try {
    await api.post(`/api/v1/domains/${domainId.value}/waf/ip-lists`, {
      list_id: bindListId.value,
      enabled: true,
    })
    toast.success('已引用 IP 名单')
    bindListOpen.value = false
    await loadIps()
  } catch (e: any) {
    toast.error(e?.data?.error || '引用失败')
  } finally {
    bindSaving.value = false
  }
}

async function unbindIpList(b: any) {
  if (!domainId.value) return
  const ok = await ask({
    title: '解除 IP 名单引用',
    message: `确定解除对「${b.list_name || '该名单'}」的引用？站点将不再套用该名单规则。`,
    confirmText: '解除',
    danger: true,
  })
  if (!ok) return
  try {
    await api.del(`/api/v1/domains/${domainId.value}/waf/ip-lists/${b.id}`)
    toast.success('已解除引用')
    await loadIps()
  } catch (e: any) {
    toast.error(e?.data?.error || '解除失败')
  }
}

const GEO_KIND_LABELS: Record<string, string> = {
  country_allow: '地区白名单',
  country_deny: '地区黑名单',
}

function geoKindLabel(kind: string) {
  return GEO_KIND_LABELS[kind] || kind
}

function geoKindTone(kind: string) {
  return kind.endsWith('_allow') ? 'success' : 'danger'
}

/** e.g. 「亚洲（全部）、美国」 */
function geoValueLabel(rule: any, max = 6) {
  const continents: string[] = rule.continents || []
  const countries: string[] = rule.countries || []
  if (!continents.length && !countries.length) return rule.value
  return geo.areaLabel(continents, countries, max)
}

async function loadGeo() {
  if (!domainId.value) return
  try {
    const [rules] = await Promise.all([
      api.get<any[]>(`/api/v1/domains/${domainId.value}/waf/geo`),
      geo.load(),
    ])
    geoRules.value = rules || []
  } catch (e: any) {
    geoRules.value = []
    toast.error(e?.data?.error || e?.message || '加载地区名单失败')
  }
}

async function loadEvents() {
  if (!domainId.value) return
  eventsLoading.value = true
  try {
    const q = new URLSearchParams({
      domain: domains.value.find((d) => d.id === domainId.value)?.name || '',
      limit: String(eventLimit.value),
    })
    events.value = (await api.get<any[]>(`/api/v1/waf/events?${q.toString()}`)) || []
  } catch (e: any) {
    events.value = []
    toast.error(e?.data?.error || e?.message || '加载 WAF 事件失败')
  } finally {
    eventsLoading.value = false
  }
}

async function loadBans() {
  bansError.value = ''
  if (!currentDomainName.value) {
    bans.value = []
    ccStats.value = null
    return
  }
  bansLoading.value = true
  try {
    const q = new URLSearchParams()
    q.set('domain', currentDomainName.value)
    q.set('limit', '500')
    const suffix = `?${q.toString()}`
    const res = await api.get<{ bans: any[] }>(`/api/v1/cc/bans/history${suffix}`)
    bans.value = res.bans || []
    if (auth.isAdmin) {
      const statsSuffix = `?domain=${encodeURIComponent(currentDomainName.value)}`
      ccStats.value = await api.get<any>(`/api/v1/cc/stats${statsSuffix}`).catch(() => null)
    } else {
      ccStats.value = null
    }
  } catch (e: any) {
    bans.value = []
    ccStats.value = null
    const status = e?.response?.status ?? e?.status
    bansError.value =
      status === 404
        ? '控制面未提供封禁历史接口，请升级并重启 lagran-control。'
        : status >= 500
          ? '加载封禁历史失败，请检查控制面日志和数据库迁移是否完成。'
          : friendlyErrorMessage(e?.data?.error, '加载封禁历史失败，请稍后重试')
  } finally {
    bansLoading.value = false
  }
}

function openBanModal() {
  banError.value = ''
  banForm.value = {
    domain: currentDomainName.value,
    ip: '',
    seconds: 3600,
    reason: 'manual-cc-ban',
  }
  banModalOpen.value = true
}

async function saveBan() {
  banError.value = ''
  if (!banForm.value.domain.trim()) {
    banError.value = '请选择域名'
    return
  }
  if (!banForm.value.ip.trim()) {
    banError.value = '请填写 IP 或 CIDR'
    return
  }
  saving.value = true
  try {
    await api.post('/api/v1/cc/bans/manual', {
      domain: banForm.value.domain.trim(),
      ip: banForm.value.ip.trim(),
      seconds: Number(banForm.value.seconds) || 3600,
      reason: banForm.value.reason.trim(),
    })
    banModalOpen.value = false
    toast.success('已添加共享封禁，边缘约 5 秒内同步')
    await loadBans()
  } catch (e: any) {
    banError.value = e?.data?.error || '添加封禁失败'
  } finally {
    saving.value = false
  }
}

function banStatusLabel(status: string) {
  if (status === 'active') return '生效中'
  if (status === 'revoked') return '已解封'
  return '已过期'
}

function banTone(status: string) {
  if (status === 'active') return 'danger'
  if (status === 'revoked') return 'info'
  return 'neutral'
}

function remainingLabel(seconds: number) {
  const s = Math.max(0, Number(seconds) || 0)
  if (s < 60) return `${s} 秒`
  if (s < 3600) return `${Math.ceil(s / 60)} 分钟`
  return `${Math.floor(s / 3600)} 小时 ${Math.ceil((s % 3600) / 60)} 分`
}

async function unbanIp(ban: any) {
  const ok = await ask({
    title: '解除共享封禁',
    message: `确认解封 ${ban.domain} 的 ${ban.ip}？所有边缘节点会在下一轮同步时解除。`,
    confirmText: '解封',
    danger: true,
  })
  if (!ok) return
  try {
    await api.post('/api/v1/cc/bans/unban', {
      domain: ban.domain,
      ip: ban.ip,
    })
    toast.success('已解封，边缘节点将在约 5 秒内同步')
    await loadBans()
  } catch (e: any) {
    toast.error(e?.data?.error || '解封失败')
  }
}

async function reload() {
  loading.value = true
  try {
    await loadDomains()
    await loadRulesets()
    await loadRulepacks()
    await Promise.all([loadSettings(), loadRules(), loadIps(), loadGeo(), loadBans()])
    if (section.value === 'events') await loadEvents()
    if (section.value === 'overview') await loadStats()
  } finally {
    loading.value = false
  }
}

async function enterSection(next: Tab) {
  if (next === 'events') await loadEvents()
  if (next === 'rules') await loadRules()
  if (next === 'ips') await loadIps()
  if (next === 'geo') await loadGeo()
  if (next === 'bans') await loadBans()
  if (next === 'settings') await loadSettings()
  if (next === 'overview') await loadStats()
}

watch(section, (next) => { void enterSection(next) })

async function saveSettings() {
  if (!domainId.value) return
  saving.value = true
  try {
    const s = await api.put<any>(`/api/v1/domains/${domainId.value}/waf`, settings.value)
    settings.value = {
      enabled: !!s.enabled,
      mode: s.mode === 'monitor' ? 'monitor' : 'block',
      sqli: !!s.sqli,
      xss: !!s.xss,
      traversal: !!s.traversal,
      scanners: !!s.scanners,
      rce: !!s.rce,
      ssrf: !!s.ssrf,
      xxe: !!s.xxe,
      log4shell: !!s.log4shell,
      crlf: !!s.crlf,
      ssti: !!s.ssti,
      nosql: !!s.nosql,
      ldap: !!s.ldap,
      java: !!s.java,
      webshell: !!s.webshell,
      protocol: !!s.protocol,
      security_level: s.security_level || 'off',
      challenge_difficulty: s.challenge_difficulty || 4,
      challenge_ttl_secs: s.challenge_ttl_secs || 1800,
      managed_rulesets: Array.isArray(s.managed_rulesets) ? s.managed_rulesets : [],
      waf_rulepack_version: s.waf_rulepack_version || '',
      waf_gray_percent: s.waf_gray_percent ?? 0,
    }
    toast.success('WAF 设置已保存，边缘约 15 秒内同步')
  } catch (e: any) {
    toast.error(e?.data?.error || '保存失败')
  } finally {
    saving.value = false
  }
}

function openRuleModal(rule?: any) {
  ruleError.value = ''
  editingRule.value = rule || null
  ruleForm.value = rule
    ? {
      name: rule.name,
      field: rule.field && rule.field.startsWith('header:') ? 'header' : rule.field,
      header_name: rule.field && rule.field.startsWith('header:') ? rule.field.slice(7) : '',
      operator: rule.operator,
      value: rule.value,
      action: rule.action,
      priority: rule.priority,
      enabled: rule.enabled,
      expr: rule.expr || '',
    }
    : {
      name: '',
      field: 'path',
      header_name: '',
      operator: 'contains',
      value: '',
      action: 'block',
      priority: 0,
      enabled: true,
      expr: '',
    }
  ruleModalOpen.value = true
}

const dslTextarea = ref<HTMLTextAreaElement | null>(null)
const dslSuggestOpen = ref(false)
const dslSuggestions = ref<{ label: string; insert: string; detail: string; back?: number }[]>([])
const dslSuggestIndex = ref(0)
const dslTokenStart = ref(0)

const DSL_FIELDS = [
  { label: 'path', insert: 'path ', detail: '请求路径' },
  { label: 'query', insert: 'query ', detail: '查询串' },
  { label: 'user_agent', insert: 'user_agent ', detail: 'User-Agent' },
  { label: 'referer', insert: 'referer ', detail: 'Referer' },
  { label: 'method', insert: 'method ', detail: '请求方法' },
  { label: 'ip', insert: 'ip ', detail: '客户端 IP' },
  { label: 'header:', insert: 'header:', detail: '请求头' },
]
const DSL_OPS = [
  { label: 'contains', insert: 'contains ', detail: '包含' },
  { label: 'equals', insert: 'equals ', detail: '等于' },
  { label: 'prefix', insert: 'prefix ', detail: '前缀' },
  { label: 'suffix', insert: 'suffix ', detail: '后缀' },
]
const DSL_KEYWORDS = [
  { label: 'AND', insert: 'AND ', detail: '同时满足' },
  { label: 'OR', insert: 'OR ', detail: '任一满足' },
  { label: 'NOT', insert: 'NOT ', detail: '取反' },
]

function dslFieldKeys() {
  return DSL_FIELDS.map((f) => f.label.replace(':', ''))
}
function dslOpKeys() {
  return [...DSL_OPS.map((o) => o.label), 'in']
}
function insideDslString(before: string) {
  let quote = ''
  let escaped = false
  for (const ch of before) {
    if (escaped) {
      escaped = false
      continue
    }
    if (ch === '\\') {
      escaped = true
      continue
    }
    if (quote) {
      if (ch === quote) quote = ''
    } else if (ch === '"' || ch === "'") {
      quote = ch
    }
  }
  return !!quote
}

function updateDslSuggestions() {
  const el = dslTextarea.value
  if (!el) return
  const value = ruleForm.value.expr || ''
  const caret = el.selectionStart ?? value.length
  const before = value.slice(0, caret)
  if (insideDslString(before)) {
    dslSuggestOpen.value = false
    return
  }
  const tokenMatch = before.match(/[A-Za-z_][A-Za-z0-9_:-]*$/)
  const token = tokenMatch?.[0] ?? ''
  const start = tokenMatch ? caret - token.length : caret
  dslTokenStart.value = start
  const prior = before.slice(0, start).trimEnd()
  const prev = (prior.match(/[A-Za-z_][A-Za-z0-9_:-]*$/) || [])[0]?.toLowerCase() || ''
  const fields = dslFieldKeys()
  const ops = dslOpKeys()
  let candidates: { label: string; insert: string; detail: string; back?: number }[] = []

  if (fields.includes(prev) || prev.startsWith('header:')) {
    candidates = prev === 'ip'
      ? [...DSL_OPS, { label: 'in', insert: 'in ', detail: 'CIDR 网段' }]
      : DSL_OPS
  } else if (ops.includes(prev)) {
    candidates = prev === 'in'
      ? [{ label: '"10.0.0.0/8"', insert: '"10.0.0.0/8"', detail: 'CIDR' }]
      : [{ label: '""', insert: '""', detail: '字符串', back: 1 }]
  } else if (prev === 'and' || prev === 'or' || prev === 'not' || prior === '' || prior.endsWith('(')) {
    candidates = [...DSL_FIELDS, ...DSL_KEYWORDS]
  } else if (prior.endsWith(')') || prior.endsWith('"') || prior.endsWith("'")) {
    candidates = DSL_KEYWORDS
  } else {
    candidates = [...DSL_FIELDS, ...DSL_KEYWORDS]
  }

  const lower = token.toLowerCase()
  dslSuggestions.value = candidates.filter((item) =>
    item.label.toLowerCase().startsWith(lower)
  )
  dslSuggestIndex.value = 0
  dslSuggestOpen.value = dslSuggestions.value.length > 0
}

function closeDslSuggestionsSoon() {
  window.setTimeout(() => {
    dslSuggestOpen.value = false
  }, 120)
}

function applyDslSuggestion(item: { label: string; insert: string; back?: number }) {
  const el = dslTextarea.value
  if (!el) return
  const value = ruleForm.value.expr || ''
  const caret = el.selectionStart ?? value.length
  const start = dslTokenStart.value
  const next = value.slice(0, start) + item.insert + value.slice(caret)
  ruleForm.value.expr = next
  dslSuggestOpen.value = false
  nextTick(() => {
    const pos = start + item.insert.length - (item.back || 0)
    el.focus()
    el.setSelectionRange(pos, pos)
  })
}

function onDslKeydown(event: KeyboardEvent) {
  if (!dslSuggestOpen.value || !dslSuggestions.value.length) return
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    dslSuggestIndex.value = (dslSuggestIndex.value + 1) % dslSuggestions.value.length
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    dslSuggestIndex.value =
      (dslSuggestIndex.value - 1 + dslSuggestions.value.length) % dslSuggestions.value.length
  } else if (event.key === 'Enter' || event.key === 'Tab') {
    event.preventDefault()
    const suggestion = dslSuggestions.value[dslSuggestIndex.value]
    if (suggestion) applyDslSuggestion(suggestion)
  } else if (event.key === 'Escape') {
    dslSuggestOpen.value = false
  }
}

async function saveRule() {
  ruleError.value = ''
  const usingDsl = !!ruleForm.value.expr.trim()
  if (!ruleForm.value.name.trim() || (!usingDsl && !ruleForm.value.value.trim())) {
    ruleError.value = usingDsl ? '请填写规则名称' : '请填写规则名称与匹配内容'
    return
  }
  if (!usingDsl && ruleForm.value.field === 'header' && !ruleForm.value.header_name.trim()) {
    ruleError.value = '请填写要匹配的请求头名称'
    return
  }
  saving.value = true
  try {
    const { header_name, ...rest } = ruleForm.value
    const body = {
      ...rest,
      field: ruleForm.value.field === 'header' ? `header:${header_name.trim()}` : ruleForm.value.field,
    }
    if (editingRule.value) {
      await api.put(`/api/v1/waf/rules/${editingRule.value.id}`, body)
      toast.success('规则已更新')
    } else {
      await api.post(`/api/v1/domains/${domainId.value}/waf/rules`, body)
      toast.success('规则已创建')
    }
    ruleModalOpen.value = false
    await loadRules()
  } catch (e: any) {
    ruleError.value = e?.data?.error || '保存失败'
  } finally {
    saving.value = false
  }
}

async function toggleRule(rule: any) {
  try {
    await api.put(`/api/v1/waf/rules/${rule.id}`, {
      name: rule.name,
      field: rule.field,
      operator: rule.operator,
      value: rule.value,
      action: rule.action,
      priority: rule.priority,
      enabled: !rule.enabled,
      expr: rule.expr || '',
    })
    await loadRules()
  } catch (e: any) {
    toast.error(e?.data?.error || '更新失败')
  }
}

async function removeRule(rule: any) {
  const ok = await ask({
    title: '删除 WAF 规则',
    message: `确认删除「${rule.name}」？删除后立即生效（边缘约 15 秒内同步）。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  try {
    await api.del(`/api/v1/waf/rules/${rule.id}`)
    toast.success('规则已删除')
    await loadRules()
  } catch (e: any) {
    toast.error(e?.data?.error || '删除失败')
  }
}

function openIpModal(kind: 'allow' | 'deny', ip?: any) {
  ipError.value = ''
  editingIp.value = ip || null
  ipForm.value = ip
    ? { kind: ip.kind, ip: ip.ip, note: ip.note || '', enabled: ip.enabled }
    : { kind, ip: '', note: '', enabled: true }
  ipModalOpen.value = true
}

async function saveIp() {
  ipError.value = ''
  if (!ipForm.value.ip.trim()) {
    ipError.value = '请填写 IP 或 CIDR'
    return
  }
  saving.value = true
  try {
    if (editingIp.value) {
      await api.put(`/api/v1/waf/ips/${editingIp.value.id}`, ipForm.value)
      toast.success('IP 名单已更新')
    } else {
      await api.post(`/api/v1/domains/${domainId.value}/waf/ips`, ipForm.value)
      toast.success('IP 名单已添加')
    }
    ipModalOpen.value = false
    editingIp.value = null
    await loadIps()
  } catch (e: any) {
    ipError.value = e?.data?.error || '保存失败，请检查 IP 格式'
  } finally {
    saving.value = false
  }
}

async function toggleIp(ip: any) {
  try {
    await api.put(`/api/v1/waf/ips/${ip.id}`, {
      kind: ip.kind,
      ip: ip.ip,
      note: ip.note || '',
      enabled: !ip.enabled,
    })
    await loadIps()
  } catch (e: any) {
    toast.error(e?.data?.error || '更新失败')
  }
}

async function removeIp(ip: any) {
  const ok = await ask({
    title: '删除 IP 名单',
    message: `确认删除 ${ip.ip}（${ip.kind === 'allow' ? '白名单' : '黑名单'}）？`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  try {
    await api.del(`/api/v1/waf/ips/${ip.id}`)
    toast.success('已删除')
    await loadIps()
  } catch (e: any) {
    toast.error(e?.data?.error || '删除失败')
  }
}

async function openGeoModal(action: 'allow' | 'deny', rule?: any) {
  geoError.value = ''
  editingGeo.value = rule || null
  await geo.load()
  geoForm.value = rule
    ? {
      action: rule.kind.endsWith('_allow') ? 'allow' : 'deny',
      countries: geo.expand(rule.continents || [], rule.countries || []),
      note: rule.note || '',
      enabled: rule.enabled,
    }
    : { action, countries: [], note: '', enabled: true }
  geoModalOpen.value = true
}

async function saveGeo() {
  geoError.value = ''
  const form = geoForm.value
  if (!form.countries.length) {
    geoError.value = '请至少勾选一个洲或国家/地区'
    return
  }
  // Whole continents travel as continent codes, so the rule keeps covering
  // the continent as the catalog grows.
  const { continents, countries } = geo.collapse(form.countries)
  saving.value = true
  try {
    const body = {
      kind: `country_${form.action}`,
      continents,
      countries,
      note: form.note,
      enabled: form.enabled,
    }
    if (editingGeo.value) {
      await api.put(`/api/v1/waf/geo/${editingGeo.value.id}`, body)
      toast.success('地区规则已更新')
    } else {
      await api.post(`/api/v1/domains/${domainId.value}/waf/geo`, body)
      toast.success('地区规则已添加')
    }
    geoModalOpen.value = false
    editingGeo.value = null
    await loadGeo()
  } catch (e: any) {
    geoError.value = e?.data?.error || '保存失败，请稍后重试'
  } finally {
    saving.value = false
  }
}

async function toggleGeo(rule: any) {
  try {
    await api.put(`/api/v1/waf/geo/${rule.id}`, {
      kind: rule.kind,
      value: rule.value,
      note: rule.note || '',
      enabled: !rule.enabled,
    })
    await loadGeo()
  } catch (e: any) {
    toast.error(e?.data?.error || '更新失败')
  }
}

async function removeGeo(rule: any) {
  const ok = await ask({
    title: '删除地区规则',
    message: `确认删除 ${geoKindLabel(rule.kind)} ${geoValueLabel(rule)}？`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  try {
    await api.del(`/api/v1/waf/geo/${rule.id}`)
    toast.success('已删除')
    await loadGeo()
  } catch (e: any) {
    toast.error(e?.data?.error || '删除失败')
  }
}

watch(domainId, async (id) => {
  if (!id) return
  loading.value = true
  try {
    await Promise.all([loadSettings(), loadRules(), loadIps(), loadGeo(), loadBans()])
    if (section.value === 'events') await loadEvents()
    if (section.value === 'overview') await loadStats()
  } finally {
    loading.value = false
  }
})

watch(eventLimit, () => {
  // action is client-filtered; only limit changes need a refetch
  if (section.value === 'events') loadEvents()
})

watch(
  () => String(route.query.domain || ''),
  (q) => {
    if (opts?.domainId) return
    const id = q.trim()
    if (id && domains.value.some((d) => d.id === id) && domainId.value !== id) {
      domainId.value = id
    }
  },
)

onMounted(reload)

  return {
    actionCount,
    barHeight,
    loadStats,
    fieldLabel,
    opLabel,
    actionLabel,
    actionTone,
    eventLabel,
    eventTone,
    categoryLabel,
    loadDomains,
    loadSettings,
    loadRulesets,
    slotLabel,
    formatRulepackTime,
    loadRulepacks,
    publishRulepack,
    retireRulepack,
    deleteRulepack,
    rollbackRulepack,
    toggleRuleset,
    loadRules,
    loadIps,
    openBindListModal,
    confirmBindList,
    unbindIpList,
    geoKindLabel,
    geoKindTone,
    geoValueLabel,
    loadGeo,
    loadEvents,
    loadBans,
    openBanModal,
    saveBan,
    banStatusLabel,
    banTone,
    remainingLabel,
    unbanIp,
    reload,
    enterSection,
    saveSettings,
    openRuleModal,
    dslFieldKeys,
    dslOpKeys,
    insideDslString,
    updateDslSuggestions,
    closeDslSuggestionsSoon,
    applyDslSuggestion,
    onDslKeydown,
    saveRule,
    toggleRule,
    removeRule,
    openIpModal,
    saveIp,
    toggleIp,
    removeIp,
    openGeoModal,
    saveGeo,
    toggleGeo,
    removeGeo,
    api,
    toast,
    auth,
    route,
    domains,
    domainId,
    section,
    loading,
    saving,
    stats,
    statsLoading,
    securityLevels,
    levelHint,
    settings,
    rules,
    rulesets,
    rulepackVersions,
    rulepackBusy,
    ips,
    ipBindings,
    availableIpLists,
    bindListOpen,
    bindListId,
    bindSaving,
    geoRules,
    geoPage,
    geoPageSize,
    pagedGeo,
    rulePage,
    ipPage,
    rulePageSize,
    ipPageSize,
    pagedRules,
    pagedIps,
    events,
    eventsLoading,
    eventAction,
    eventLimit,
    bans,
    bansLoading,
    bansError,
    banStatusFilter,
    currentDomainName,
    filteredBans,
    ccStats,
    ruleModalOpen,
    editingRule,
    ruleError,
    ruleForm,
    ipModalOpen,
    ipError,
    editingIp,
    ipForm,
    geoModalOpen,
    geoError,
    editingGeo,
    geoForm,
    banModalOpen,
    banError,
    banForm,
    managedRules,
    tabs,
    maxDayCount,
    filteredEvents,
    eventPage,
    eventPageSize,
    pagedEvents,
    dslTextarea,
    dslSuggestOpen,
    dslSuggestions,
    dslSuggestIndex,
    dslTokenStart,
    DSL_FIELDS,
    DSL_OPS,
    DSL_KEYWORDS,
  }
}
