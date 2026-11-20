<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">添加站点</div>
        <div class="page-desc">先确认域名和接入方式，看到 NS 或 CNAME 后再填源站。一个站点可以绑定多个域名</div>
      </div>
      <NuxtLink to="/domains" class="btn btn-default">
        <AppIcon name="chevron-left" :size="15" />
        返回站点管理
      </NuxtLink>
    </div>

    <div class="wizard-steps">
      <div v-for="(s, i) in steps" :key="s" class="wizard-step" :class="{ active: step === i, done: step > i }">
        <span class="wizard-index">{{ step > i ? '✓' : i + 1 }}</span>
        <span>{{ s }}</span>
      </div>
    </div>

    <div class="card">
      <div class="card-body">
        <div v-if="step === 0" class="form-col">
          <div class="wizard-hero">
            <div class="wizard-icon"><AppIcon name="domain" :size="24" /></div>
            <div>
              <div class="wizard-title">站点要加速哪些域名？</div>
              <div class="wizard-desc">
                第一个为主域名；可继续添加别名。支持一级泛域名（如 <code>*.example.com</code>），精确主机名优先于泛匹配。
              </div>
            </div>
          </div>
          <div class="field">
            <label class="field-label">主域名 <span class="req">*</span></label>
            <input v-model="form.name" class="input" placeholder="cdn.example.com 或 *.example.com" @keyup.enter="next" />
            <span class="field-hint">
              只填写主机名，不带 https:// 或路径。泛域名仅支持前导单标签（<code>*.example.com</code>），
              不能写 <code>*.*.example.com</code>；多级子域需单独注册或使用更长的泛名（如 <code>*.cdn.example.com</code>）。
            </span>
          </div>
          <div class="field">
            <label class="field-label">站点名称（可选）</label>
            <input
              v-model="form.display_name"
              class="input"
              maxlength="64"
              placeholder="例如：官网 CDN、活动站"
              @keyup.enter="next"
            />
            <span class="field-hint">仅用于控制台展示，不影响域名解析。留空则显示主域名。</span>
          </div>
          <div class="field">
            <label class="field-label">更多域名（可选）</label>
            <div v-for="(alias, i) in form.aliases" :key="i" class="alias-row">
              <input v-model="form.aliases[i]" class="input" placeholder="www.example.com 或 *.example.com" />
              <button class="btn btn-text btn-sm danger" type="button" @click="form.aliases.splice(i, 1)">
                移除
              </button>
            </div>
            <button class="btn btn-default btn-sm" type="button" style="margin-top: 8px" @click="form.aliases.push('')">
              <AppIcon name="plus" :size="14" />
              添加域名
            </button>
          </div>
        </div>

        <div v-else-if="step === 1" class="form-col">
          <div class="wizard-hero">
            <div class="wizard-icon"><AppIcon name="cert" :size="24" /></div>
            <div>
              <div class="wizard-title">选择接入方式</div>
              <div class="wizard-desc">
                CNAME 适合精确主机名；整域托管可自动签发根域与泛域名证书。CNAME 若要泛域名证书，需另绑 DNS 服务商账号。
              </div>
            </div>
          </div>

          <div class="access-grid">
            <button
              type="button"
              class="access-card"
              :class="{ active: form.access_mode === 'cname' }"
              @click="form.access_mode = 'cname'"
            >
              <strong>CNAME 接入</strong>
              <span>不用改 NS；主机名 CNAME 到 CDN 主域后，精确名走 HTTP-01 自动签发。泛域名需在「智能 DNS → DNS 服务商」绑定客户 DNS 账号。</span>
            </button>
            <button
              type="button"
              class="access-card"
              :class="{ active: form.access_mode === 'ns' }"
              @click="form.access_mode = 'ns'"
            >
              <strong>整域托管</strong>
              <span>在注册商把 DNS 服务器改到平台；自动创建托管域名，平台直接写 TXT，自动签发根域与泛域名证书。</span>
            </button>
          </div>

        </div>

        <div v-else-if="step === 2" class="form-col">
          <div class="wizard-hero">
            <div class="wizard-icon"><AppIcon name="cert" :size="24" /></div>
            <div>
              <div class="wizard-title">
                {{ form.access_mode === 'ns' ? '把 NS 改到平台，并检测是否生效' : '把域名 CNAME 到下面的地址' }}
              </div>
              <div class="wizard-desc">
                {{
                  form.access_mode === 'ns'
                    ? '进入本步时会先在平台创建托管域名，再检测注册商 NS 是否已指向平台。未生效也可先继续。'
                    : '在你自己的 DNS 里添加下面的记录。根域名如果不能做 CNAME，就改成 A 记录指向节点 IP。'
                }}
              </div>
            </div>
          </div>

          <div v-if="form.access_mode === 'cname'" class="dns-box">
            <div class="dns-path-title">CNAME 地址</div>
            <template v-if="cnameTarget">
              <div v-for="host in plannedHosts" :key="host" class="dns-row">
                <span class="dns-type">{{ recordType(host) }}</span>
                <code>{{ recordName(host) }}</code>
                <span class="text-muted">→</span>
                <code>{{ recordTarget(host) }}</code>
                <button class="btn btn-text btn-sm" type="button" @click="copy(recordTarget(host))">复制</button>
              </div>
            </template>
            <div v-else-if="nodeIps.length" class="dns-row">
              <span class="dns-type">A</span>
              <code>{{ form.name }}</code>
              <span class="text-muted">→</span>
              <code>{{ nodeIps[0] }}</code>
              <button class="btn btn-text btn-sm" type="button" @click="copy(nodeIps[0])">复制</button>
            </div>
            <div v-else class="alert alert-warning">
              <AppIcon name="alert" :size="16" class="icon" />
              <span>暂无 CDN 主域或在线节点。请管理员在「智能 DNS → CDN 主域」完成配置。</span>
            </div>
            <div class="status-row" style="margin-top: 8px">
              <span>解析检测</span>
              <strong :class="preview?.dns_ok ? 'ok' : 'pending'">{{ preview?.detail || (previewing ? '检测中…' : '等待检测') }}</strong>
            </div>
          </div>

          <div v-else class="dns-box">
            <div class="dns-path-title">整域托管（根域：{{ preview?.apex || inferredApex }}）</div>
            <div v-if="(preview?.ns_servers || []).length" class="ns-list">
              <div v-for="ns in preview?.ns_servers || []" :key="ns" class="dns-row">
                <span class="dns-type">DNS</span>
                <code>{{ ns }}</code>
                <button class="btn btn-text btn-sm" type="button" @click="copy(ns)">复制</button>
              </div>
            </div>
            <div v-else class="alert alert-warning">
              <AppIcon name="alert" :size="16" class="icon" />
              <span>平台尚未配置 DNS 服务器。请先在「智能 DNS → CDN 主域」完成配置。</span>
            </div>
            <div v-if="nsPrepared" class="text-muted" style="font-size: 12px; margin-top: 4px">
              平台托管域已就绪（{{ preview?.apex || inferredApex }}）
            </div>
            <div class="status-row" style="margin-top: 8px">
              <span>NS 检测</span>
              <strong :class="preview?.ns_ready ? 'ok' : 'pending'">{{ preview?.ns_ready ? '已指向平台' : (preview?.detail || (previewing ? '检测中…' : '等待检测')) }}</strong>
            </div>
            <div v-if="(preview?.observed_ns || []).length" class="text-muted" style="font-size: 12px; margin-top: 6px">
              公网当前 NS：{{ preview.observed_ns.join('、') }}
            </div>
          </div>

          <div class="flex-between">
            <span class="text-muted" style="font-size: 12px">页面会自动重新检测。DNS 还没改完也可以先继续填写源站。</span>
            <button class="btn btn-default" type="button" :disabled="previewing || nsPreparing" @click="refreshPreview">
              {{ previewing || nsPreparing ? '检测中…' : '立即检测' }}
            </button>
          </div>
        </div>

        <div v-else-if="step === 3" class="form-col">
          <div class="wizard-hero">
            <div class="wizard-icon"><AppIcon name="node" :size="24" /></div>
            <div>
              <div class="wizard-title">源站、HTTPS 和访问日志</div>
              <div class="wizard-desc">DNS 记录可以稍后再生效。这里决定边缘从哪取内容，以及要不要记访问日志。</div>
            </div>
          </div>
          <div class="form-grid">
            <div class="field">
              <label class="field-label">源站协议</label>
              <select v-model="form.origin_proto" class="select">
                <option value="https">HTTPS</option>
                <option value="http">HTTP</option>
              </select>
            </div>
            <div class="field">
              <label class="field-label">源站端口</label>
              <input v-model.number="form.origin_port" class="input" type="number" />
            </div>
          </div>
          <div class="field">
            <label class="field-label">源站主机 <span class="req">*</span></label>
            <input v-model="form.origin_host" class="input" placeholder="origin.example.com 或 203.0.113.10" />
            <span class="field-hint">支持 IP、域名；端口需对边缘节点网络可达。</span>
          </div>
          <div class="field">
            <label class="field-label">源站 TLS SNI（可选）</label>
            <input v-model="form.origin_sni" class="input" placeholder="例如 origin.example.com" />
          </div>
          <div class="field">
            <label class="field-label">回源 Host（可选）</label>
            <input v-model="form.origin_host_header" class="input" placeholder="留空则使用访客访问的域名" />
            <span class="field-hint">源站是对象存储等第三方域名时，填源站要求的域名。</span>
          </div>
          <label class="check">
            <input v-model="form.ssl_enabled" type="checkbox" />
            开启 HTTPS（HTTP 请求将跳转到 HTTPS）
          </label>
          <label class="check">
            <input v-model="form.auto_https" type="checkbox" />
            自动 HTTPS（DNS 就绪后自动签发 Let's Encrypt）
          </label>
          <label class="check">
            <input v-model="form.access_log_enabled" type="checkbox" />
            记录访问日志（关闭后这个站点不再写入访问日志）
          </label>
          <div class="field">
            <label class="field-label">所属项目</label>
            <select v-model="form.project_id" class="select">
              <option value="">自动归入个人项目</option>
              <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select>
          </div>
        </div>

        <div v-else class="form-col">
          <div class="wizard-hero">
            <div class="wizard-icon"><AppIcon name="check" :size="24" /></div>
            <div>
              <div class="wizard-title">完成 DNS 并等待证书</div>
              <div class="wizard-desc">
                {{ form.access_mode === 'ns' ? '把注册商的 DNS 服务器改为下方平台地址，等待生效后自动签发证书。' : '把主机名 CNAME 到 CDN 主域，诊断通过后可自动签发证书。' }}
              </div>
            </div>
          </div>

          <div v-if="domainId" class="result-box">
            <div class="result-title">站点已创建</div>
            <div v-if="form.display_name.trim()" style="font-weight: 600; margin-bottom: 4px">{{ form.display_name.trim() }}</div>
            <div class="mono">{{ form.name }}</div>
            <div v-if="createdHostnames.length > 1" class="text-muted" style="font-size: 12px; margin-top: 6px">
              全部域名：{{ createdHostnames.join('、') }}
            </div>
            <div class="text-muted" style="font-size: 12px; margin-top: 4px">ID: {{ domainId }}</div>
          </div>

          <div v-if="form.access_mode === 'cname'" class="dns-box">
            <div class="dns-path-title">CNAME 接入</div>
            <div v-if="access?.cname_target || endpoints.cname" class="dns-row">
              <span class="dns-type">CNAME</span>
              <code>{{ form.name }}</code>
              <span class="text-muted">→</span>
              <code>{{ access?.cname_target || endpoints.cname }}</code>
              <button class="btn btn-text btn-sm" type="button" @click="copy(access?.cname_target || endpoints.cname)">复制</button>
            </div>
            <div v-else-if="nodeIps.length" class="dns-row">
              <span class="dns-type">A</span>
              <code>{{ form.name }}</code>
              <span class="text-muted">→</span>
              <code>{{ nodeIps[0] }}</code>
              <button class="btn btn-text btn-sm" type="button" @click="copy(nodeIps[0])">复制</button>
            </div>
            <div v-else class="alert alert-warning">
              <AppIcon name="alert" :size="16" class="icon" />
              <span>暂无 CDN 主域或在线节点。请管理员在「智能 DNS → CDN 主域」完成配置。</span>
            </div>
          </div>

          <div v-else class="dns-box">
            <div class="dns-path-title">整域托管（根域：{{ access?.apex || inferredApex }}）</div>
            <div class="text-muted" style="font-size: 12px; line-height: 1.55; margin-bottom: 10px">
              在域名注册商将 DNS 服务器改为以下平台地址。托管域名与 CDN 加速记录已自动创建。
            </div>
            <div v-if="(access?.ns_servers || []).length" class="ns-list">
              <div v-for="ns in access?.ns_servers || []" :key="ns" class="dns-row">
                <span class="dns-type">DNS</span>
                <code>{{ ns }}</code>
                <button class="btn btn-text btn-sm" type="button" @click="copy(ns)">复制</button>
              </div>
            </div>
            <div v-else class="alert alert-warning">
              <AppIcon name="alert" :size="16" class="icon" />
              <span>平台尚未配置 DNS 服务器。请先在「智能 DNS → CDN 主域」完成配置。</span>
            </div>
          </div>

          <div class="status-panel">
            <div class="status-row">
              <span>DNS</span>
              <strong :class="access?.dns_ok ? 'ok' : 'pending'">{{ access?.dns_ok ? '已就绪' : '等待生效' }}</strong>
            </div>
            <div class="status-row">
              <span>证书</span>
              <strong :class="certClass">{{ certLabel }}</strong>
            </div>
            <div v-if="access?.cert_last_error" class="text-danger" style="font-size: 12px; margin-top: 6px">
              {{ access.cert_last_error }}
            </div>
          </div>

          <div class="flex-between">
            <span class="text-muted" style="font-size: 12px">DNS 传播通常需要几分钟；页面会自动刷新状态。</span>
            <div style="display: flex; gap: 8px">
              <button class="btn btn-default" type="button" :disabled="refreshing" @click="refreshAccess">
                {{ refreshing ? '刷新中…' : '刷新状态' }}
              </button>
              <button class="btn btn-default" type="button" :disabled="diagnosing || !domainId" @click="diagnose">
                {{ diagnosing ? '诊断中…' : '立即诊断' }}
              </button>
              <button
                class="btn btn-primary"
                type="button"
                :disabled="issuing || !domainId || !form.auto_https"
                @click="retryHttps"
              >
                {{ issuing ? '签发中…' : '重试自动 HTTPS' }}
              </button>
            </div>
          </div>

          <div v-if="diag" class="diag-grid">
            <div v-for="c in diag.checks" :key="c.key" class="diag-card" :class="{ bad: !c.ok }">
              <div class="diag-head">
                <AppIcon :name="c.ok ? 'check' : 'close'" :size="16" />
                <strong>{{ c.label }}</strong>
                <span class="text-muted">{{ c.latency_ms }} ms</span>
              </div>
              <div class="diag-detail">{{ c.detail }}</div>
            </div>
          </div>
        </div>

        <p v-if="error" class="text-danger" style="font-size: 13px">{{ error }}</p>

        <div class="wizard-foot">
          <button v-if="step > 0 && step < 4" class="btn btn-default" type="button" @click="step -= 1">上一步</button>
          <span class="spacer" />
          <button v-if="step < 3" class="btn btn-primary" type="button" :disabled="!canNext" @click="next">
            下一步
          </button>
          <button v-else-if="step === 3" class="btn btn-primary" type="button" :disabled="submitting || !canNext" @click="createDomain">
            {{ submitting ? '创建中…' : '创建站点' }}
          </button>
          <NuxtLink v-if="step === 4" to="/domains" class="btn btn-primary">完成，返回站点列表</NuxtLink>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'

const api = useApi()
const toast = useToast()

const steps = ['添加域名', '接入方式', 'DNS 配置', '源站与 HTTPS', '完成']
const step = ref(0)
const submitting = ref(false)
const nsPreparing = ref(false)
const nsPrepared = ref(false)
const diagnosing = ref(false)
const refreshing = ref(false)
const issuing = ref(false)
const error = ref('')
const projects = ref<any[]>([])
const endpoints = ref<{ cname: string; ips: string[]; nodes: any[] }>({
  cname: '',
  ips: [],
  nodes: [],
})
const domainId = ref('')
const createdHostnames = ref<string[]>([])
const diag = ref<any>(null)
const access = ref<any>(null)
const preview = ref<any>(null)
const previewing = ref(false)
let pollTimer: ReturnType<typeof setInterval> | null = null

const form = ref({
  name: '',
  display_name: '',
  aliases: [] as string[],
  origin_host: '',
  origin_port: 443,
  origin_proto: 'https',
  origin_sni: '',
  origin_host_header: '',
  ssl_enabled: true,
  auto_https: true,
  access_mode: 'cname' as 'cname' | 'ns',
  access_log_enabled: true,
  project_id: '',
})

/** Exact FQDN or a leading one-label wildcard (*.example.com). */
function isValidHostname(raw: string): boolean {
  const name = raw.trim().toLowerCase().replace(/\.$/, '')
  if (!name || name.length > 253) return false
  if (name.startsWith('*.')) {
    return /^\*\.([a-z0-9]([a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}$/i.test(name)
  }
  if (name.includes('*')) return false
  return /^([a-z0-9]([a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}$/i.test(name)
}

const canNext = computed(() => {
  if (step.value === 0) {
    if (!isValidHostname(form.value.name)) return false
    return form.value.aliases.every((a) => !a.trim() || isValidHostname(a))
  }
  if (step.value === 3) return !!form.value.origin_host.trim() && form.value.origin_port > 0
  return true
})

const plannedHosts = computed(() => {
  const names = [
    form.value.name.trim().toLowerCase(),
    ...form.value.aliases.map((a) => a.trim().toLowerCase()).filter(Boolean),
  ]
  return [...new Set(names.filter(Boolean))]
})

const cnameTarget = computed(() => preview.value?.cname_target || endpoints.value.cname || '')

function isApexHost(host: string) {
  const name = host.trim().toLowerCase().replace(/\.$/, '')
  if (!name || name.startsWith('*.')) return false
  const parts = name.split('.').filter(Boolean)
  return parts.length <= 2
}

function recordType(host: string) {
  if (!cnameTarget.value) return 'A'
  // 根域通常不能做 CNAME，有节点 IP 时给出 A 记录。
  if (isApexHost(host) && nodeIps.value.length) return 'A'
  return 'CNAME'
}

function recordName(host: string) {
  return host
}

function recordTarget(host: string) {
  if (recordType(host) === 'A') return nodeIps.value[0] || ''
  return cnameTarget.value
}

const nodeIps = computed(() => endpoints.value.ips)

const inferredApex = computed(() => {
  let host = form.value.name.trim().toLowerCase().replace(/\.$/, '')
  if (host.startsWith('*.')) host = host.slice(2)
  const parts = host.split('.').filter(Boolean)
  if (parts.length <= 2) return host
  return `${parts[parts.length - 2]}.${parts[parts.length - 1]}`
})

const certLabel = computed(() => {
  const s = access.value?.cert_status || 'none'
  if (s === 'active') return '已签发'
  if (s === 'pending') return '签发中 / 等待 DNS'
  if (s === 'error') return '签发失败'
  return '未签发'
})

const certClass = computed(() => {
  const s = access.value?.cert_status || 'none'
  if (s === 'active') return 'ok'
  if (s === 'error') return 'bad'
  return 'pending'
})

async function next() {
  error.value = ''
  if (!canNext.value) {
    error.value =
      step.value === 0
        ? '请输入有效域名（支持 *.example.com；主域名必填；别名留空可跳过）'
        : '请填写源站主机和端口'
    return
  }
  step.value += 1
  if (step.value === 2) {
    if (form.value.access_mode === 'ns') {
      const ok = await prepareNsZone()
      if (!ok) {
        step.value = 1
        return
      }
    }
    await refreshPreview()
    startPoll()
  }
}

/** Write the apex into platform DNS before NS detection (hosted zone + glue). */
async function prepareNsZone() {
  nsPreparing.value = true
  nsPrepared.value = false
  try {
    const hostnames = [
      form.value.name.trim().toLowerCase(),
      ...form.value.aliases.map((a) => a.trim().toLowerCase()).filter(Boolean),
    ]
    await api.post('/api/v1/domains/ns-prepare', {
      hostname: hostnames[0],
      hostnames,
      project_id: form.value.project_id || undefined,
    })
    nsPrepared.value = true
    return true
  } catch (e: any) {
    const message = friendlyErrorMessage(e?.data?.error, '预创建托管域名失败')
    error.value = message
    toast.error(message)
    return false
  } finally {
    nsPreparing.value = false
  }
}

async function createDomain() {
  error.value = ''
  submitting.value = true
  try {
    const hostnames = [
      form.value.name.trim().toLowerCase(),
      ...form.value.aliases.map((a) => a.trim().toLowerCase()).filter(Boolean),
    ]
    const row = await api.post<any>('/api/v1/domains', {
      hostnames,
      name: hostnames[0],
      display_name: form.value.display_name.trim() || undefined,
      origin_host: form.value.origin_host.trim(),
      origin_port: form.value.origin_port,
      origin_proto: form.value.origin_proto,
      ssl_enabled: form.value.ssl_enabled,
      origin_sni: form.value.origin_sni.trim(),
      origin_host_header: form.value.origin_host_header.trim(),
      project_id: form.value.project_id || undefined,
      access_mode: form.value.access_mode,
      auto_https: form.value.auto_https,
      access_log_enabled: form.value.access_log_enabled,
    })
    domainId.value = row.id
    createdHostnames.value = row.hostnames?.length ? row.hostnames : hostnames
    step.value = 4
    await loadNodes()
    await refreshAccess()
    startPoll()
  } catch (e: any) {
    const message = friendlyErrorMessage(e?.data?.error, '创建站点失败，请稍后重试')
    error.value = message
    toast.error(message)
  } finally {
    submitting.value = false
  }
}

async function diagnose() {
  if (!domainId.value) return
  diagnosing.value = true
  try {
    diag.value = await api.post<any>(`/api/v1/domains/${domainId.value}/diagnose`, {})
  } catch (e: any) {
    toast.error(
      `${e?.data?.error || '诊断失败'}；请检查 CNAME / NS 是否已按指引配置`
    )
  } finally {
    diagnosing.value = false
  }
}

async function refreshAccess() {
  if (!domainId.value) return
  refreshing.value = true
  try {
    access.value = await api.get<any>(`/api/v1/domains/${domainId.value}/access-status`)
  } catch {
    access.value = null
  } finally {
    refreshing.value = false
  }
}

async function retryHttps() {
  if (!domainId.value) return
  issuing.value = true
  try {
    const res = await api.post<any>(`/api/v1/domains/${domainId.value}/auto-https`, {})
    if (res?.ok) {
      // Issuance runs in the background worker; the status poll picks up the result.
      toast.success('已提交申请，证书签发通常需要 1–3 分钟')
    } else {
      toast.error(res?.error || '提交失败')
    }
    await refreshAccess()
  } catch (e: any) {
    toast.error(e?.data?.error || '签发失败')
  } finally {
    issuing.value = false
  }
}

async function refreshPreview(quiet = false) {
  const hostname = form.value.name.trim().toLowerCase()
  if (!hostname) return
  previewing.value = true
  try {
    preview.value = await api.post<any>('/api/v1/domains/access-preview', {
      hostname,
      access_mode: form.value.access_mode,
    })
  } catch (e: any) {
    if (!quiet) toast.error(e?.data?.error || 'DNS 检测失败')
  } finally {
    previewing.value = false
  }
}

function startPoll() {
  stopPoll()
  pollTimer = setInterval(() => {
    if (step.value === 2) {
      refreshPreview(true)
    } else if (step.value === 4 && domainId.value) {
      refreshAccess()
    }
  }, 15000)
}

function stopPoll() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

async function loadNodes() {
  try {
    const data = await api.get<any>('/api/v1/edge-endpoints')
    endpoints.value = {
      cname: data?.cname || '',
      ips: data?.ips || [],
      nodes: data?.nodes || [],
    }
  } catch {
    endpoints.value = { cname: '', ips: [], nodes: [] }
  }
}

function copy(text: string) {
  if (!import.meta.client || !text) return
  navigator.clipboard?.writeText(text).then(() => toast.success('已复制')).catch(() => {})
}

onMounted(async () => {
  try {
    projects.value = (await api.get<any[]>('/api/v1/projects')) || []
  } catch {
    projects.value = []
  }
  await loadNodes()
})

onBeforeUnmount(() => stopPoll())
</script>

<style scoped>
.wizard-steps {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
  flex-wrap: wrap;
}

.wizard-step {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  border-radius: 20px;
  background: var(--card);
  border: 1px solid var(--border);
  font-size: 13px;
  color: var(--text-3);
}

.wizard-step.active {
  border-color: var(--primary);
  color: var(--text);
}

.wizard-step.done {
  color: var(--success, #16a34a);
}

.wizard-index {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-2, #f3f4f6);
  font-size: 12px;
}

.wizard-hero {
  display: flex;
  gap: 12px;
  margin-bottom: 16px;
}

.wizard-icon {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-2, #f3f4f6);
}

.wizard-title {
  font-size: 16px;
  font-weight: 600;
}

.wizard-desc {
  font-size: 13px;
  color: var(--text-3);
  margin-top: 4px;
}

.form-col {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.alias-row {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.access-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.access-card {
  text-align: left;
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 14px;
  background: var(--card);
  cursor: pointer;
}

.access-card.active {
  border-color: var(--primary);
  box-shadow: 0 0 0 1px var(--primary);
}

.access-card strong {
  display: block;
  margin-bottom: 6px;
}

.access-card span {
  font-size: 12px;
  color: var(--text-3);
  line-height: 1.5;
}

.dns-box {
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 14px;
  background: var(--bg-1, #fafafa);
}

.dns-path-title {
  font-weight: 600;
  margin-bottom: 10px;
}

.dns-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  margin-bottom: 8px;
  font-size: 13px;
}

.dns-type {
  font-size: 11px;
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--bg-2, #eee);
}

.status-panel {
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 12px 14px;
}

.status-row {
  display: flex;
  justify-content: space-between;
  font-size: 13px;
  margin-bottom: 6px;
}

.status-row .ok {
  color: var(--success, #16a34a);
}

.status-row .pending {
  color: var(--text-3);
}

.status-row .bad {
  color: var(--danger, #dc2626);
}

.result-box {
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 12px 14px;
}

.result-title {
  font-weight: 600;
  margin-bottom: 4px;
}

.wizard-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 18px;
}

.spacer {
  flex: 1;
}

.diag-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.diag-card {
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 10px;
}

.diag-card.bad {
  border-color: var(--danger, #dc2626);
}

.diag-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
}

.diag-detail {
  font-size: 12px;
  color: var(--text-3);
  margin-top: 6px;
}

@media (max-width: 720px) {
  .form-grid,
  .access-grid,
  .diag-grid {
    grid-template-columns: 1fr;
  }
}
</style>
