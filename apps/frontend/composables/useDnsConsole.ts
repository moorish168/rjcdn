import { friendlyErrorMessage } from '~/utils/friendlyError'
import {
  countriesFromDnsLine,
  ensureDnsLineForCountries,
} from '~/composables/ensureDnsLine'

export type DnsTab = 'settings' | 'zones' | 'providers' | 'nodes' | 'lines'

export interface GeoRegionDto {
  id: string
  code: string
  name: string
  kind: string
  country: string
  province: string
  city: string
  isp: string
  continent: string
  /** Comma-separated ISO country codes. */
  countries: string
  is_default: boolean
  enabled: boolean
  sort_order: number
  note: string
}

export function useDnsConsole() {
const api = useApi()
const auth = useAuthStore()
const toast = useToast()
const { ask } = useConfirm()
const route = useRoute()

interface NsEntry {
  name: string
  ip: string
}
interface DnsSettings {
  configured: boolean
  zone: string
  ns: NsEntry[]
  ttl: number
  fallback_ip: string
  enabled: boolean
  serial: number
  /** Version a DNS node reports once it has pulled the current config. */
  sync_serial: number
}
interface ResolverCheck {
  key: string
  name: string
  state: 'ok' | 'stale' | 'no_answer' | 'wrong_ns' | 'unreachable' | 'not_found' | 'error' | 'skipped'
  ns: string[]
  soa_serial: number | null
  sample_ip: string | null
  detail: string
  comment: string
}
interface PublicCheck {
  zone: string
  expected_ns: string[]
  expected_serial: number
  probe_name: string
  verdict: 'ok' | 'partial' | 'fail' | 'unknown'
  summary: string
  resolvers: ResolverCheck[]
  checked_at: string
}
interface ZoneDto {
  id: string
  name: string
  kind: string
  serial: number
  enabled: boolean
  ttl_default: number
  /** Bound CDN site when created via 整域托管. */
  site_id?: string | null
  site_name?: string | null
}
interface RecordDto {
  id: string
  zone_id: string
  name: string
  rtype: string
  content: string
  ttl: number
  priority: number | null
  cdn_proxied: boolean
  enabled: boolean
  line_id?: string | null
  line_code?: string | null
  line_name?: string | null
  managed_by_node_id?: string | null
  managed_node_name?: string | null
  managed_by_site_id?: string | null
  /** Site-managed records: owning site and the origin edges use. */
  site_id?: string | null
  site_name?: string | null
  origin?: string | null
}

const section = computed<DnsTab>(() => {
  // The static host serves /dns/zones as /dns/zones/: an empty last segment
  // made a reload or a direct link fall back to the settings tab's data.
  const raw = route.path.split('/').filter(Boolean).pop() || 'zones'
  const allowed: DnsTab[] = ['settings', 'zones', 'providers', 'nodes', 'lines']
  return (allowed.includes(raw as DnsTab) ? raw : 'zones') as DnsTab
})
const lines = ref<GeoRegionDto[]>([])
const linesLoading = ref(false)
const lineOptions = computed(() => lines.value.filter((l) => l.enabled && !l.is_default))
const lineOpen = ref(false)
const lineSaving = ref(false)
const editingLine = ref<GeoRegionDto | null>(null)
const geo = useGeoCatalog()
const lineForm = reactive({
  code: '',
  name: '',
  kind: 'custom',
  country: '',
  province: '',
  city: '',
  isp: '',
  continent: '',
  /** Selected ISO country codes for 洲→国家勾选. */
  countries: [] as string[],
  is_default: false,
  enabled: true,
  note: '',
})

function parseCountriesCsv(raw: string | undefined | null): string[] {
  if (!raw) return []
  return raw
    .split(/[,;\s]+/)
    .map((s) => s.trim().toUpperCase())
    .filter((s) => /^[A-Z]{2}$/.test(s))
}

function lineMatchLabel(l: GeoRegionDto): string {
  const parts: string[] = []
  const countries = parseCountriesCsv(l.countries)
  if (countries.length) {
    const folded = geo.collapse(countries)
    parts.push(geo.areaLabel(folded.continents, folded.countries, 4))
  } else if (l.continent) {
    parts.push(`${geo.continentName(l.continent)}（全部）`)
  } else if (l.country) {
    parts.push(geo.countryName(l.country.toUpperCase()))
  }
  if (l.province) parts.push(l.province)
  if (l.city) parts.push(l.city)
  if (l.isp) parts.push(l.isp)
  return parts.join(' / ') || '—'
}

// The picker edits `lineForm.countries`; keep the continent hint (set when
// every pick sits in one continent) and the line kind in step with it.
watch(
  () => lineForm.countries,
  (countries) => {
    const owners = new Set(countries.map(geo.continentOf).filter(Boolean))
    lineForm.continent = owners.size === 1 ? [...owners][0] : ''
    syncLineKindFromGeo()
  },
)

function syncLineKindFromGeo() {
  // User-managed lines stay `custom` so DNS picks them over seeded
  // country / province / isp lines. Only the default line uses kind=default.
  lineForm.kind = lineForm.is_default ? 'default' : 'custom'
}
const loading = ref(false)
const saving = ref(false)
const error = ref('')
const form = reactive({
  zone: '',
  ttl: 30,
  fallback_ip: '',
  enabled: true,
  ns: [{ name: '', ip: '' }] as NsEntry[],
})

const zonesLoading = ref(false)
const zones = ref<ZoneDto[]>([])
const selectedZone = ref<ZoneDto | null>(null)
const recordsLoading = ref(false)
const records = ref<RecordDto[]>([])

interface DnsNodeDto {
  id: string
  name: string
  public_ip: string
  status: string
  version: string
  last_serial: number | null
  last_seen_at: string | null
  healthy: boolean
  enabled: boolean
  /** CDN 主域 NS names pointing at this server's IP. */
  ns_names?: string[]
}

/**
 * Names to show for a DNS node: every NS name on its IP (ns1 and ns2 on one
 * address are one server, so both belong on its row), else its own name.
 */
function nodeNames(n: { name: string; ns_names?: string[] }): string[] {
  return n.ns_names?.length ? n.ns_names : [n.name]
}

// Whether each DNS node has pulled the config saved on the CDN 主域 page.
type NodeSyncState = 'synced' | 'syncing' | 'offline' | 'unreported' | 'disabled'
const syncStateLabels: Record<NodeSyncState, { text: string; tone: 'success' | 'info' | 'danger' | 'warning' | 'neutral' }> = {
  synced: { text: '已生效', tone: 'success' },
  syncing: { text: '同步中', tone: 'info' },
  offline: { text: '离线', tone: 'danger' },
  unreported: { text: '未上报', tone: 'warning' },
  disabled: { text: '已停用', tone: 'neutral' },
}
const syncSerial = ref(0)
const syncNodes = ref<DnsNodeDto[]>([])
const syncLoaded = ref(false)
let syncTimer: ReturnType<typeof setTimeout> | null = null
let syncPollUntil = 0

function nodeSyncState(n: DnsNodeDto): NodeSyncState {
  if (n.enabled === false || n.status === 'disabled') return 'disabled'
  if (!n.healthy) return 'offline'
  // No heartbeat yet: the node runs but cannot tell which config it has.
  if (n.last_serial == null) return 'unreported'
  return n.last_serial === syncSerial.value ? 'synced' : 'syncing'
}
const syncRows = computed(() => syncNodes.value.map((n) => ({ ...n, sync: nodeSyncState(n) })))

// What the public internet sees (Google / Cloudflare / AliDNS over DoH).
const checkStateLabels: Record<ResolverCheck['state'], { text: string; tone: 'success' | 'info' | 'danger' | 'warning' | 'neutral' }> = {
  ok: { text: '正常', tone: 'success' },
  stale: { text: '缓存旧配置', tone: 'warning' },
  no_answer: { text: '没有调度结果', tone: 'warning' },
  wrong_ns: { text: 'DNS 服务器不一致', tone: 'danger' },
  unreachable: { text: '联系不上', tone: 'danger' },
  not_found: { text: '域名不存在', tone: 'danger' },
  error: { text: '异常', tone: 'danger' },
  skipped: { text: '跳过', tone: 'neutral' },
}
const verdictTones: Record<PublicCheck['verdict'], 'success' | 'warning' | 'danger' | 'neutral'> = {
  ok: 'success',
  partial: 'warning',
  fail: 'danger',
  unknown: 'neutral',
}
const publicCheck = ref<PublicCheck | null>(null)
const publicChecking = ref(false)
const publicCheckError = ref('')
interface RemoteDnsNode {
  id: string
  name: string
  public_ip: string
  ssh_host: string
  ssh_port: number
  grant_id: string | null
  grant_name: string
  version: string
  last_op: { id: string; action: string; status: string; error: string } | null
}
interface SshGrant {
  id: string
  name: string
  username: string
}
const nodesLoading = ref(false)
const dnsNodes = ref<DnsNodeDto[]>([])
const remoteDnsMap = ref<Record<string, RemoteDnsNode>>({})
const sshGrants = ref<SshGrant[]>([])
const nodeOpen = ref(false)
const nodeSaving = ref(false)
const nodeError = ref('')
const nodeForm = reactive({ name: '', public_ip: '', notes: '' })
const remoteOpen = ref(false)
const remoteSaving = ref(false)
const remoteBusy = ref(false)
const remoteError = ref('')
const remoteNode = ref<DnsNodeDto | null>(null)
const remoteForm = reactive({ host: '', port: 22, grant_id: '' })
const dnsRemoteActions = [
  { key: 'install', label: '安装', danger: false },
  { key: 'upgrade', label: '升级', danger: false },
  { key: 'reinstall', label: '重装', danger: true },
  { key: 'start', label: '启动', danger: false },
  { key: 'stop', label: '停止', danger: true },
  { key: 'uninstall', label: '卸载', danger: true },
] as const
const dnsOpLogOpen = ref(false)
const dnsOpLogText = ref('')
const testLoading = ref(false)
const testError = ref('')
const testResult = ref<any>(null)
const testForm = reactive({ name: '', server: '', ip: '' })

const dnsNodeRows = computed(() =>
  dnsNodes.value.map((n) => {
    const r = remoteDnsMap.value[n.id]
    return {
      ...n,
      ssh_host: r?.ssh_host || '',
      ssh_port: r?.ssh_port || 22,
      grant_id: r?.grant_id || null,
      last_op: r?.last_op || null,
      version: r?.version || n.version,
    }
  }),
)

function dnsActionLabel(action: string) {
  return dnsRemoteActions.find((a) => a.key === action)?.label || action
}
function dnsOpStatusLabel(status: string) {
  return ({ queued: '排队中', running: '执行中', success: '成功', failed: '失败' } as Record<string, string>)[status] || status
}
function dnsOpTone(status: string) {
  return ({ queued: 'neutral', running: 'info', success: 'success', failed: 'danger' } as Record<string, any>)[status] || 'neutral'
}

const zoneOpen = ref(false)
const zoneSaving = ref(false)
const zoneError = ref('')
const zoneForm = reactive({ name: '' })

const recordOpen = ref(false)
const recordSaving = ref(false)
const recordError = ref('')
const editingRecord = ref<RecordDto | null>(null)
const recordTypes = ['A', 'AAAA', 'CNAME', 'TXT', 'MX', 'NS']
const recordForm = reactive({
  name: '@',
  rtype: 'A',
  content: '',
  ttl: 30,
  priority: 10,
  cdn_proxied: false,
  /** Kept for edit of ISP/province-only lines that have no country list. */
  line_id: '',
  /** ISO country codes picked on the record form (洲 → 国家). */
  countries: [] as string[],
})

function countriesFromLine(l: GeoRegionDto | undefined | null): string[] {
  return countriesFromDnsLine(l, geo.continentCountries)
}

async function ensureLineForCountries(countries: string[]): Promise<string | null> {
  const id = await ensureDnsLineForCountries(api, countries, '由解析记录自动创建')
  await loadLines()
  return id
}

function recordLineLabel(r: RecordDto): string {
  // NULL line_id and the system default geo_region are the same catch-all
  // at resolve time; always show one label so CDN 兜底 and「不选地区」match.
  const fallback = '默认线路'
  if (!r.line_id) return fallback
  const line = lines.value.find((l) => l.id === r.line_id)
  if (!line) {
    const name = (r.line_name || '').trim()
    if (!name || name === '默认' || name === '默认线路') return fallback
    return name
  }
  if (line.is_default || line.kind === 'default') return fallback
  const area = lineMatchLabel(line)
  return area === '—' ? line.name || r.line_name || fallback : area
}

const nsHostnames = computed(() => form.ns.map((n) => n.name.trim()).filter(Boolean))
const delegationLines = computed(() => {
  const zone = form.zone.trim().replace(/\.$/, '')
  if (!zone) return [] as string[]
  const lines: string[] = []
  for (const row of form.ns) {
    const name = row.name.trim().replace(/\.$/, '')
    const ip = row.ip.trim()
    if (name && ip) lines.push(`DNS Host: ${name}  →  ${ip}`)
  }
  for (const row of form.ns) {
    const name = row.name.trim().replace(/\.$/, '')
    if (name) lines.push(`${zone}.  NS  ${name}.`)
  }
  return lines
})
const recordPlaceholder = computed(() => {
  switch (recordForm.rtype) {
    case 'A':
      return '203.0.113.10'
    case 'AAAA':
      return '2001:db8::1'
    case 'CNAME':
      return 'target.example.com'
    case 'TXT':
      return 'v=spf1 ...'
    case 'MX':
      return 'mail.example.com'
    case 'NS':
      return 'ns1.example.com'
    default:
      return ''
  }
})

function autofillNs() {
  const zone = form.zone.trim().replace(/\.$/, '').toLowerCase()
  if (!zone) return
  if (form.ns.length === 1 && !form.ns[0].name.trim()) form.ns[0].name = `ns1.${zone}`
  form.ns.forEach((row) => {
    if (row.name && !row.name.includes('.')) row.name = `${row.name}.${zone}`
  })
}
function addNs() {
  const zone = form.zone.trim().replace(/\.$/, '').toLowerCase()
  form.ns.push({
    name: zone ? `ns${form.ns.length + 1}.${zone}` : '',
    ip: form.ns[0]?.ip || '',
  })
}
function removeNs(idx: number) {
  if (form.ns.length <= 1) return
  form.ns.splice(idx, 1)
}

async function loadSettings() {
  loading.value = true
  error.value = ''
  try {
    const data = await api.get<DnsSettings>('/api/v1/dns/settings')
    form.zone = data.zone || ''
    form.ttl = data.ttl || 30
    form.fallback_ip = data.fallback_ip || ''
    form.enabled = data.enabled !== false
    form.ns = data.ns?.length
      ? data.ns.map((n) => ({ name: n.name, ip: n.ip }))
      : [{ name: data.zone ? `ns1.${data.zone}` : '', ip: '' }]
    syncSerial.value = data.sync_serial || 0
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '读取 DNS 配置失败'))
  } finally {
    loading.value = false
  }
  await loadSyncNodes()
}

async function loadSyncNodes() {
  try {
    syncNodes.value = await api.get<DnsNodeDto[]>('/api/v1/dns/nodes')
  } catch {
    // The status card is informational; the settings form must still work.
  } finally {
    syncLoaded.value = true
  }
}

/** Re-read the expected version and every node's reported one. */
async function refreshSyncStatus() {
  try {
    const [settings, nodes] = await Promise.all([
      api.get<DnsSettings>('/api/v1/dns/settings'),
      api.get<DnsNodeDto[]>('/api/v1/dns/nodes'),
    ])
    syncSerial.value = settings.sync_serial || 0
    syncNodes.value = nodes
  } catch {
    // Keep the last known state; the next poll or 刷新 retries.
  } finally {
    syncLoaded.value = true
  }
}

function stopSyncPolling() {
  if (syncTimer) clearTimeout(syncTimer)
  syncTimer = null
}

// Nodes pull every 15 s and report right after, so a minute covers every
// online node; polling stops early once none is still catching up.
function startSyncPolling() {
  stopSyncPolling()
  syncPollUntil = Date.now() + 60_000
  void pollSync()
}

async function pollSync() {
  await refreshSyncStatus()
  const pending = syncRows.value.some((r) => r.sync === 'syncing')
  syncTimer = pending && Date.now() < syncPollUntil ? setTimeout(pollSync, 5000) : null
}

onBeforeUnmount(stopSyncPolling)

async function runPublicCheck() {
  publicChecking.value = true
  publicCheckError.value = ''
  try {
    publicCheck.value = await api.get<PublicCheck>('/api/v1/dns/settings/public-check')
  } catch (e: any) {
    publicCheck.value = null
    publicCheckError.value = friendlyErrorMessage(e?.data?.error || e?.message || e, '检测失败')
  } finally {
    publicChecking.value = false
  }
}

async function saveSettings() {
  error.value = ''
  if (!form.zone.trim()) {
    error.value = '请填写 CDN 主域'
    return
  }
  saving.value = true
  try {
    const data = await api.put<DnsSettings>('/api/v1/dns/settings', {
      zone: form.zone.trim(),
      ttl: form.ttl,
      fallback_ip: form.fallback_ip.trim() || null,
      enabled: form.enabled,
      ns: form.ns
        .filter((n) => n.name.trim() && n.ip.trim())
        .map((n) => ({ name: n.name.trim(), ip: n.ip.trim() })),
    })
    form.zone = data.zone
    form.ttl = data.ttl
    form.fallback_ip = data.fallback_ip || ''
    form.enabled = data.enabled
    form.ns = data.ns.map((n) => ({ name: n.name, ip: n.ip }))
    syncSerial.value = data.sync_serial || 0
    // An earlier external check describes the old settings.
    publicCheck.value = null
    publicCheckError.value = ''
    toast.success('CDN 主域已保存，DNS 节点会在 15 秒内同步')
    startSyncPolling()
  } catch (e: any) {
    error.value = friendlyErrorMessage(e?.data?.error || e?.message || e, '保存失败')
  } finally {
    saving.value = false
  }
}

async function loadZones() {
  zonesLoading.value = true
  try {
    zones.value = await api.get<ZoneDto[]>('/api/v1/dns/zones')
    if (selectedZone.value) {
      const still = zones.value.find((z) => z.id === selectedZone.value!.id)
      selectedZone.value = still || null
      if (still) await loadRecords()
      else records.value = []
    }
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '读取域名失败'))
  } finally {
    zonesLoading.value = false
  }
}

async function openZones() {
  await navigateTo({ path: '/dns/zones', query: route.query })
  await loadZones()
}

async function selectZone(z: ZoneDto) {
  selectedZone.value = z
  await loadRecords()
}

async function loadRecords() {
  if (!selectedZone.value) return
  recordsLoading.value = true
  try {
    records.value = await api.get<RecordDto[]>(
      `/api/v1/dns/zones/${selectedZone.value.id}/records`,
    )
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '读取记录失败'))
  } finally {
    recordsLoading.value = false
  }
}

function openCreateZone() {
  zoneForm.name = ''
  zoneError.value = ''
  zoneOpen.value = true
}

async function createZone() {
  zoneError.value = ''
  if (!zoneForm.name.trim()) {
    zoneError.value = '请填写域名'
    return
  }
  zoneSaving.value = true
  try {
    const z = await api.post<ZoneDto>('/api/v1/dns/zones', {
      name: zoneForm.name.trim(),
      kind: 'hosted',
    })
    zoneOpen.value = false
    toast.success('域名已添加')
    await loadZones()
    selectedZone.value = z
    await loadRecords()
  } catch (e: any) {
    zoneError.value = friendlyErrorMessage(e?.data?.error || e?.message || e, '创建失败')
  } finally {
    zoneSaving.value = false
  }
}

async function removeZone(z: ZoneDto) {
  if (
    !(await ask({
      title: '删除域名',
      message: `确认删除「${z.name}」及其全部记录？`,
      confirmText: '删除',
      danger: true,
    }))
  ) {
    return
  }
  try {
    await api.del(`/api/v1/dns/zones/${z.id}`)
    toast.success('托管域名已删除')
    if (selectedZone.value?.id === z.id) {
      selectedZone.value = null
      records.value = []
    }
    await loadZones()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '删除失败'))
  }
}

async function loadLines() {
  linesLoading.value = true
  try {
    // Names for the 「匹配」 column come from the shared catalog.
    void geo.load()
    lines.value = (await api.get<GeoRegionDto[]>('/api/v1/geo-regions')) || []
  } catch (e: any) {
    lines.value = []
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '加载自定义线路失败'))
  } finally {
    linesLoading.value = false
  }
}

async function openLines() {
  await navigateTo({ path: '/dns/lines', query: route.query })
  await loadLines()
}

async function openCreateLine() {
  editingLine.value = null
  await geo.load()
  Object.assign(lineForm, {
    code: '',
    name: '',
    kind: 'custom',
    country: '',
    province: '',
    city: '',
    isp: '',
    continent: '',
    countries: [],
    is_default: false,
    enabled: true,
    note: '',
  })
  lineOpen.value = true
}

async function openEditLine(l: GeoRegionDto) {
  editingLine.value = l
  await geo.load()
  let countries = parseCountriesCsv(l.countries)
  // A whole-continent line stores the continent alone; show it fully ticked.
  if (!countries.length && l.continent) countries = geo.continentCountries(l.continent)
  if (!countries.length && l.country) countries.push(l.country.toUpperCase())
  Object.assign(lineForm, {
    code: l.code,
    name: l.name,
    kind: l.kind,
    country: l.country,
    province: l.province,
    city: l.city,
    isp: l.isp,
    continent: l.continent || '',
    countries,
    is_default: l.is_default,
    enabled: l.enabled,
    note: l.note,
  })
  lineOpen.value = true
}

async function saveLine() {
  lineSaving.value = true
  try {
    syncLineKindFromGeo()
    const countries = [...lineForm.countries].sort()
    // Exactly one whole continent is stored as the continent itself: DNS
    // matches it against the continent GeoLite2 reports for the visitor, so
    // the line keeps covering every country of it.
    const folded = geo.collapse(countries)
    const wholeContinent =
      folded.continents.length === 1 &&
      !folded.countries.length &&
      !lineForm.province.trim() &&
      !lineForm.city.trim() &&
      !lineForm.isp.trim()
    const payload = {
      code: lineForm.code,
      name: lineForm.name,
      // Keep kind=custom (or default) so pick_region ranks user lines above
      // seeded 标准线路; continent/country fields still drive the match.
      kind: lineForm.is_default ? 'default' : 'custom',
      country: wholeContinent ? '' : countries.length === 1 ? countries[0] : (lineForm.country || ''),
      province: lineForm.province,
      city: lineForm.city,
      isp: lineForm.isp,
      continent: wholeContinent ? folded.continents[0] : lineForm.continent,
      countries: wholeContinent ? [] : countries,
      is_default: lineForm.is_default,
      enabled: lineForm.enabled,
      note: lineForm.note,
    }
    if (editingLine.value) {
      await api.put(`/api/v1/geo-regions/${editingLine.value.id}`, payload)
      toast.success('自定义线路已更新')
    } else {
      await api.post('/api/v1/geo-regions', payload)
      toast.success('自定义线路已创建')
    }
    lineOpen.value = false
    await loadLines()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '保存自定义线路失败'))
  } finally {
    lineSaving.value = false
  }
}

async function openCreateRecord() {
  editingRecord.value = null
  recordForm.name = '@'
  recordForm.rtype = 'A'
  recordForm.content = ''
  recordForm.ttl = selectedZone.value?.ttl_default || 30
  recordForm.priority = 10
  recordForm.cdn_proxied = false
  recordForm.line_id = ''
  recordForm.countries = []
  recordError.value = ''
  await Promise.all([loadLines(), geo.load()])
  recordOpen.value = true
}

async function openEditRecord(r: RecordDto) {
  editingRecord.value = r
  recordForm.name = r.name
  recordForm.rtype = r.rtype
  recordForm.content = r.content
  recordForm.ttl = r.ttl
  recordForm.priority = r.priority ?? 10
  recordForm.cdn_proxied = r.cdn_proxied
  recordForm.line_id = r.line_id || ''
  recordError.value = ''
  await Promise.all([loadLines(), geo.load()])
  const line = lines.value.find((l) => l.id === r.line_id)
  // ISP / province custom lines stay bound by id; geo lines re-hydrate as
  // country ticks so「解析请求来源」shows 地域.
  if (line && (line.isp?.trim() || line.province?.trim() || line.city?.trim())) {
    recordForm.countries = []
    recordForm.line_id = line.id
  } else {
    recordForm.countries = countriesFromLine(line)
    recordForm.line_id = countriesFromLine(line).length ? '' : r.line_id || ''
  }
  recordOpen.value = true
}

async function saveRecord() {
  if (!selectedZone.value) return
  recordError.value = ''
  if (!recordForm.content.trim()) {
    recordError.value = '请填写记录内容'
    return
  }
  recordSaving.value = true
  try {
    let lineId: string | null = null
    if (['A', 'AAAA', 'CNAME'].includes(recordForm.rtype)) {
      if (recordForm.countries.length) {
        lineId = await ensureLineForCountries(recordForm.countries)
      } else if (recordForm.line_id) {
        // Explicit custom / ISP line from「解析请求来源」.
        const kept = lines.value.find((l) => l.id === recordForm.line_id)
        if (kept && !kept.is_default) lineId = kept.id
      }
    }
    // No cdn_proxied: CDN acceleration of a name belongs to its site, which
    // writes and removes those records itself.
    const body: Record<string, unknown> = {
      name: recordForm.name.trim() || '@',
      rtype: recordForm.rtype,
      content: recordForm.content.trim(),
      ttl: recordForm.ttl,
      priority: recordForm.rtype === 'MX' ? recordForm.priority : null,
      line_id: lineId,
    }
    if (editingRecord.value) {
      await api.put(
        `/api/v1/dns/zones/${selectedZone.value.id}/records/${editingRecord.value.id}`,
        body,
      )
      toast.success('记录已更新')
    } else {
      await api.post(`/api/v1/dns/zones/${selectedZone.value.id}/records`, body)
      toast.success('记录已创建')
    }
    recordOpen.value = false
    await loadRecords()
    await loadZones()
  } catch (e: any) {
    recordError.value = friendlyErrorMessage(e?.data?.error || e?.message || e, '保存记录失败')
  } finally {
    recordSaving.value = false
  }
}

async function removeRecord(r: RecordDto) {
  if (!selectedZone.value) return
  if (
    !(await ask({
      title: '删除记录',
      message:
        r.cdn_proxied || r.managed_by_site_id
          ? `${r.name} 是站点「整域托管」自动维护的记录，删除后这个名字将不再由站点联动。确认删除？`
          : `确认删除 ${r.name} ${r.rtype}？`,
      confirmText: '删除',
      danger: true,
    }))
  ) {
    return
  }
  try {
    await api.del(`/api/v1/dns/zones/${selectedZone.value.id}/records/${r.id}`)
    toast.success('记录已删除')
    await loadRecords()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '删除失败'))
  }
}

async function openNodes() {
  await navigateTo({ path: '/dns/nodes', query: route.query })
  await loadNodes()
}

async function loadNodes() {
  nodesLoading.value = true
  try {
    const [nodes, remote, grants] = await Promise.all([
      api.get<DnsNodeDto[]>('/api/v1/dns/nodes'),
      api.get<RemoteDnsNode[]>('/api/v1/remote/dns-nodes').catch(() => [] as RemoteDnsNode[]),
      api.get<SshGrant[]>('/api/v1/remote/grants').catch(() => [] as SshGrant[]),
    ])
    dnsNodes.value = nodes
    remoteDnsMap.value = Object.fromEntries((remote || []).map((r) => [r.id, r]))
    sshGrants.value = grants || []
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '读取 DNS 节点失败'))
  } finally {
    nodesLoading.value = false
  }
}

function openRemote(n: DnsNodeDto) {
  const r = remoteDnsMap.value[n.id]
  remoteNode.value = n
  remoteForm.host = r?.ssh_host || n.public_ip
  remoteForm.port = r?.ssh_port || 22
  remoteForm.grant_id = r?.grant_id || sshGrants.value[0]?.id || ''
  remoteError.value = ''
  remoteOpen.value = true
}

async function ensureRemoteSsh(quiet = false): Promise<boolean> {
  if (!remoteNode.value) return false
  remoteError.value = ''
  if (!remoteForm.host.trim() || !remoteForm.grant_id) {
    remoteError.value = '请填写 SSH 地址并选择认证'
    return false
  }
  try {
    await api.put(`/api/v1/remote/dns-nodes/${remoteNode.value.id}/ssh`, {
      host: remoteForm.host.trim(),
      port: Number(remoteForm.port) || 22,
      grant_id: remoteForm.grant_id,
    })
    if (!quiet) toast.success('SSH 已保存')
    await loadNodes()
    return true
  } catch (e: any) {
    remoteError.value = friendlyErrorMessage(e?.data?.error || e?.message || e, '保存 SSH 失败')
    return false
  }
}

async function saveRemoteSsh() {
  remoteSaving.value = true
  try {
    await ensureRemoteSsh(false)
  } finally {
    remoteSaving.value = false
  }
}

async function runDnsRemote(action: string) {
  if (!remoteNode.value) return
  remoteError.value = ''
  const label = dnsActionLabel(action)
  if (['reinstall', 'uninstall', 'stop'].includes(action)) {
    if (!(await ask({
      title: `${label} DNS 节点`,
      message: `确认对「${remoteNode.value.name}」执行${label}？`,
      confirmText: label,
      danger: true,
    }))) {
      return
    }
  }
  remoteBusy.value = true
  try {
    if (!(await ensureRemoteSsh(true))) return
    const results = await api.post<{ node_id: string; op_id?: string; error?: string }[]>(
      '/api/v1/remote/dns-ops',
      { node_ids: [remoteNode.value.id], action },
    )
    const first = results?.[0]
    if (first?.error) {
      remoteError.value = first.error
      return
    }
    toast.success(`已排队：${label}`)
    if (first?.op_id) await openDnsOpLog(first.op_id)
    await loadNodes()
  } catch (e: any) {
    remoteError.value = friendlyErrorMessage(e?.data?.error || e?.message || e, `${label}失败`)
  } finally {
    remoteBusy.value = false
  }
}

async function openDnsOpLog(id: string) {
  dnsOpLogText.value = ''
  dnsOpLogOpen.value = true
  try {
    const op = await api.get<{ log?: string; status: string; error: string }>(
      `/api/v1/remote/dns-ops/${id}`,
    )
    dnsOpLogText.value = op.log || op.error || `状态：${op.status}`
    if (op.status === 'queued' || op.status === 'running') {
      setTimeout(() => {
        if (dnsOpLogOpen.value) openDnsOpLog(id)
      }, 2000)
    }
  } catch (e: any) {
    dnsOpLogText.value = friendlyErrorMessage(e?.data?.error || e?.message || e, '读取日志失败')
  }
}

function openCreateNode() {
  nodeForm.name = ''
  nodeForm.public_ip = ''
  nodeForm.notes = ''
  nodeError.value = ''
  nodeOpen.value = true
}

async function createNode() {
  nodeError.value = ''
  if (!nodeForm.name.trim() || !nodeForm.public_ip.trim()) {
    nodeError.value = '请填写名称和公网 IP'
    return
  }
  nodeSaving.value = true
  try {
    await api.post('/api/v1/dns/nodes', {
      name: nodeForm.name.trim(),
      public_ip: nodeForm.public_ip.trim(),
      notes: nodeForm.notes.trim(),
    })
    nodeOpen.value = false
    toast.success('DNS 节点已添加')
    await loadNodes()
  } catch (e: any) {
    nodeError.value = friendlyErrorMessage(e?.data?.error || e?.message || e, '添加失败')
  } finally {
    nodeSaving.value = false
  }
}

async function removeNode(n: DnsNodeDto) {
  if (
    !(await ask({
      title: '删除 DNS 节点',
      message: `确认删除「${n.name}」？`,
      confirmText: '删除',
      danger: true,
    }))
  ) {
    return
  }
  try {
    await api.del(`/api/v1/dns/nodes/${n.id}`)
    toast.success('已删除')
    await loadNodes()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message || e, '删除失败'))
  }
}

function runNodeTest(n: DnsNodeDto) {
  testForm.server = `${n.public_ip}:53`
  if (!testForm.name) testForm.name = form.zone || 'example.com'
  runTest()
}

async function runTest() {
  testError.value = ''
  testResult.value = null
  if (!testForm.name.trim()) {
    testError.value = '请填写查询名'
    return
  }
  testLoading.value = true
  try {
    testResult.value = await api.post('/api/v1/dns/test', {
      name: testForm.name.trim(),
      server: testForm.server.trim() || undefined,
      ip: testForm.ip.trim() || undefined,
    })
  } catch (e: any) {
    testError.value = friendlyErrorMessage(e?.data?.error || e?.message || e, 'DNS 测试失败')
  } finally {
    testLoading.value = false
  }
}

function formatTime(v: string | null) {
  if (!v) return '—'
  try {
    return new Date(v).toLocaleString('zh-CN')
  } catch {
    return v
  }
}

/** Optional hook registered by dns/providers.vue so top-level 刷新 covers that tab. */
const providersReloadHook = ref<null | (() => Promise<void>)>(null)

function registerProvidersReload(fn: (() => Promise<void>) | null) {
  providersReloadHook.value = fn
}

async function reload() {
  if (section.value === 'settings') await loadSettings()
  else if (section.value === 'zones') await loadZones()
  else if (section.value === 'lines') await loadLines()
  else if (section.value === 'providers') await providersReloadHook.value?.()
  else await loadNodes()
}

watch(
  section,
  async (next) => {
    if (next === 'zones') {
      await Promise.all([loadZones(), loadLines(), geo.load()])
    }
    if (next === 'nodes') await loadNodes()
    if (next === 'lines') await loadLines()
    if (next === 'settings') await loadSettings()
  },
  { immediate: true },
)

  return {
    dnsActionLabel,
    dnsOpStatusLabel,
    dnsOpTone,
    autofillNs,
    addNs,
    removeNs,
    loadSettings,
    saveSettings,
    refreshSyncStatus,
    runPublicCheck,
    nodeNames,
    syncRows,
    syncLoaded,
    syncSerial,
    syncStateLabels,
    publicCheck,
    publicChecking,
    publicCheckError,
    checkStateLabels,
    verdictTones,
    loadZones,
    openZones,
    selectZone,
    loadRecords,
    openCreateZone,
    createZone,
    removeZone,
    loadLines,
    registerProvidersReload,
    openLines,
    openCreateLine,
    openEditLine,
    saveLine,
    openCreateRecord,
    openEditRecord,
    saveRecord,
    removeRecord,
    openNodes,
    loadNodes,
    openRemote,
    ensureRemoteSsh,
    saveRemoteSsh,
    runDnsRemote,
    openDnsOpLog,
    openCreateNode,
    createNode,
    removeNode,
    runNodeTest,
    runTest,
    formatTime,
    reload,
    route,
    api,
    auth,
    toast,
    section,
    lines,
    linesLoading,
    lineOptions,
    lineOpen,
    lineSaving,
    editingLine,
    lineForm,
    lineMatchLabel,
    recordLineLabel,
    loading,
    saving,
    error,
    form,
    zonesLoading,
    zones,
    selectedZone,
    recordsLoading,
    records,
    nodesLoading,
    dnsNodes,
    remoteDnsMap,
    sshGrants,
    nodeOpen,
    nodeSaving,
    nodeError,
    nodeForm,
    remoteOpen,
    remoteSaving,
    remoteBusy,
    remoteError,
    remoteNode,
    remoteForm,
    dnsRemoteActions,
    dnsOpLogOpen,
    dnsOpLogText,
    testLoading,
    testError,
    testResult,
    testForm,
    dnsNodeRows,
    zoneOpen,
    zoneSaving,
    zoneError,
    zoneForm,
    recordOpen,
    recordSaving,
    recordError,
    editingRecord,
    recordTypes,
    recordForm,
    nsHostnames,
    delegationLines,
    recordPlaceholder,
  }
}
