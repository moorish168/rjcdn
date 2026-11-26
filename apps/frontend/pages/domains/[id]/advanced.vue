<script setup lang="ts">
const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
const reloadDomain = inject<() => Promise<void>>('reloadDomain')!

const submitting = ref(false)
const pageLoading = ref(true)
const wafLoading = ref(false)
const wafSettings = ref({ enabled: false, mode: 'monitor' as 'monitor' | 'block' })
const secretsConfigured = ref({
  signed_url_secret: false,
  signed_url_previous_secret: false,
  origin_auth_value: false,
})
const rotatingSecret = ref(false)
const showOriginAuthInput = ref(false)
const rotatedSecret = ref('')
const rotatedSecretOpen = ref(false)
const cacheBusy = ref(false)

interface ShieldNode {
  id: string
  name: string
  url: string
  enabled: boolean
}
const shieldNodes = ref<ShieldNode[]>([])
const enabledShieldNodes = computed(() => shieldNodes.value.filter((node) => node.enabled))
const dnsProviders = ref<{ id: string; name: string; provider_type: string; enabled: boolean }[]>([])
const enabledDnsProviders = computed(() => dnsProviders.value.filter((p) => p.enabled))

function sameShieldUrl(a: string, b: string) {
  return a.replace(/\/+$/, '') === b.replace(/\/+$/, '')
}

function shieldNodeLabel(node: ShieldNode) {
  const name = String(node?.name || '').trim()
  if (name) return name
  return String(node?.url || '').replace(/^https?:\/\//i, '').replace(/\/+$/, '')
}

async function loadShieldNodes() {
  try {
    shieldNodes.value = await api.get<ShieldNode[]>('/api/v1/shield-nodes')
  } catch {
    shieldNodes.value = []
  }
}

async function loadDnsProviders() {
  try {
    dnsProviders.value = (await api.get<any[]>('/api/v1/dns/providers')) || []
  } catch {
    dnsProviders.value = []
  }
}

const settings = ref({
  cache_enabled: true,
  ignore_query_string: false,
  ignore_origin: false,
  ignore_accept_language: false,
  cors_origins: '',
  hotlink_allow_referers: '',
  origin_timeout_ms: 0,
  custom_headers: '',
  shield_url: '',
  rate_limit_rps: 0,
  rate_limit_burst: 0,
  cc_enabled: false,
  cc_rps: 20,
  cc_burst: 40,
  cc_ban_after: 5,
  cc_ban_seconds: 300,
  cc_mode: 'block',
  cc_allow_ips: '',
  websocket_enabled: true,
  http2_enabled: true,
  http3_enabled: false,
  gzip_enabled: true,
  image_webp_enabled: true,
  access_log_enabled: true,
  min_tls_version: '1.2',
  access_mode: 'cname',
  dns_provider_id: '',
  auto_https: true,
  cert_status: 'none',
  signed_url_enabled: false,
  signed_url_mode: 'off',
  signed_url_paths: '',
  signed_url_secret: '',
  signed_url_previous_secret: '',
  signed_url_ttl_secs: 3600,
  origin_auth_header: '',
  origin_auth_value: '',
  hotlink_signature_enabled: false,
})

const currentShieldUnregistered = computed(
  () =>
    Boolean(settings.value.shield_url) &&
    !enabledShieldNodes.value.some((node) => sameShieldUrl(node.url, settings.value.shield_url)),
)

async function toggleCache() {
  if (cacheBusy.value || pageLoading.value) return
  const next = settings.value.cache_enabled === false
  const ok = await ask({
    title: next ? '开启站点缓存' : '关闭站点缓存',
    message: next
      ? '开启后该站点将按缓存规则读写缓存。确认继续？'
      : '关闭后所有请求不读写缓存，并立即清理该站点已有缓存。确认继续？',
    confirmText: next ? '开启缓存' : '关闭并清理',
    danger: !next,
  })
  if (!ok) return
  cacheBusy.value = true
  try {
    const s = await api.post<any>(`/api/v1/domains/${id.value}/cache`, {
      cache_enabled: next,
    })
    settings.value.cache_enabled = s.cache_enabled !== false
    toast.success(next ? '已开启站点缓存' : '已关闭缓存并下发清理')
    await reloadDomain?.()
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '切换失败')
  } finally {
    cacheBusy.value = false
  }
}

async function loadSettings() {
  pageLoading.value = true
  wafLoading.value = true
  wafSettings.value = { enabled: false, mode: 'monitor' }
  await Promise.all([loadShieldNodes(), loadDnsProviders()])
  try {
    const s = await api.get<any>(`/api/v1/domains/${id.value}/settings`)
    const cachedHeaders = String(s.cache_key_ignore_headers || '')
      .split(',')
      .map((v) => v.trim().toLowerCase())
    settings.value = {
      cache_enabled: s.cache_enabled !== false,
      ignore_query_string: !!s.ignore_query_string,
      ignore_origin: cachedHeaders.includes('origin'),
      ignore_accept_language: cachedHeaders.includes('accept-language'),
      cors_origins: s.cors_origins || '',
      hotlink_allow_referers: s.hotlink_allow_referers || '',
      origin_timeout_ms: s.origin_timeout_ms ?? 0,
      custom_headers: s.custom_headers || '',
      shield_url: (s.shield_url || '').replace(/\/+$/, ''),
      rate_limit_rps: s.rate_limit_rps || 0,
      rate_limit_burst: s.rate_limit_burst || 0,
      cc_enabled: !!s.cc_enabled,
      cc_rps: s.cc_rps ?? 20,
      cc_burst: s.cc_burst ?? 40,
      cc_ban_after: s.cc_ban_after ?? 5,
      cc_ban_seconds: s.cc_ban_seconds ?? 300,
      cc_mode: s.cc_mode || 'block',
      cc_allow_ips: s.cc_allow_ips || '',
      websocket_enabled: s.websocket_enabled !== false,
      http2_enabled: s.http2_enabled !== false,
      http3_enabled: s.http3_enabled === true,
      min_tls_version: ['1.0', '1.1', '1.2', '1.3'].includes(s.min_tls_version)
        ? s.min_tls_version
        : '1.2',
      gzip_enabled: s.gzip_enabled !== false,
      image_webp_enabled: s.image_webp_enabled !== false,
      access_log_enabled: s.access_log_enabled !== false,
      access_mode: s.access_mode === 'ns' ? 'ns' : 'cname',
      dns_provider_id: s.dns_provider_id || '',
      auto_https: s.auto_https !== false,
      cert_status: s.cert_status || 'none',
      signed_url_enabled: !!s.signed_url_enabled,
      signed_url_mode: s.signed_url_mode || (s.signed_url_enabled ? 'all' : 'off'),
      signed_url_paths: s.signed_url_paths || '',
      signed_url_secret: '',
      signed_url_previous_secret: '',
      signed_url_ttl_secs: s.signed_url_ttl_secs ?? 3600,
      origin_auth_header: s.origin_auth_header || '',
      origin_auth_value: '',
      hotlink_signature_enabled: !!s.hotlink_signature_enabled,
    }
    secretsConfigured.value = {
      signed_url_secret: !!s.signed_url_secret_set,
      signed_url_previous_secret: !!s.signed_url_previous_secret_set,
      origin_auth_value: !!s.origin_auth_value_set,
    }
    showOriginAuthInput.value = false
  } catch {
    secretsConfigured.value = {
      signed_url_secret: false,
      signed_url_previous_secret: false,
      origin_auth_value: false,
    }
    toast.error('读取高级设置失败，已使用默认值')
  }
  try {
    const w = await api.get<any>(`/api/v1/domains/${id.value}/waf`)
    wafSettings.value = {
      enabled: !!w.enabled,
      mode: w.mode === 'block' ? 'block' : 'monitor',
    }
  } catch {
    wafSettings.value = { enabled: false, mode: 'monitor' }
  } finally {
    wafLoading.value = false
    pageLoading.value = false
  }
}

function toggleOriginAuthInput() {
  showOriginAuthInput.value = !showOriginAuthInput.value
  if (!showOriginAuthInput.value) {
    settings.value.origin_auth_value = ''
  }
}

async function rotateSignedUrlSecret() {
  const hasKey = secretsConfigured.value.signed_url_secret
  const ok = await ask({
    title: hasKey ? '轮换签名密钥' : '生成签名密钥',
    message: hasKey
      ? '将生成新的签名密钥，当前密钥会作为「上一个密钥」继续被接受一段时间。新密钥只显示一次，请准备好复制。'
      : '将为该域名生成一把签名密钥。生成后「签名 URL 防护」仍是关闭状态，需要你手动开启才生效。新密钥只显示一次。',
    confirmText: hasKey ? '轮换' : '生成',
  })
  if (!ok) return
  rotatingSecret.value = true
  try {
    const res: any = await api.post(`/api/v1/domains/${id.value}/signed-url/rotate`, {})
    rotatedSecret.value = res?.signed_url_secret || ''
    rotatedSecretOpen.value = true
    secretsConfigured.value = {
      ...secretsConfigured.value,
      signed_url_secret: true,
      signed_url_previous_secret: hasKey,
    }
  } catch (e: any) {
    toast.error(e?.data?.error || '轮换失败')
  } finally {
    rotatingSecret.value = false
  }
}

async function copyRotatedSecret() {
  if (!rotatedSecret.value) return
  try {
    await navigator.clipboard.writeText(rotatedSecret.value)
    toast.success('已复制到剪贴板')
  } catch {
    toast.error('复制失败，请手动选择文本')
  }
}

function closeRotatedSecret() {
  rotatedSecretOpen.value = false
  rotatedSecret.value = ''
}

async function saveSettings() {
  submitting.value = true
  try {
    const payload: any = { ...settings.value }
    if (!payload.signed_url_secret) delete payload.signed_url_secret
    if (!payload.signed_url_previous_secret) delete payload.signed_url_previous_secret
    if (!payload.origin_auth_value) delete payload.origin_auth_value
    delete payload.signed_url_secret_set
    delete payload.origin_auth_value_set
    payload.cache_key_ignore_headers = [
      payload.ignore_origin ? 'origin' : '',
      payload.ignore_accept_language ? 'accept-language' : '',
    ]
      .filter(Boolean)
      .join(',')
    delete payload.ignore_origin
    delete payload.ignore_accept_language
    await api.put(`/api/v1/domains/${id.value}/settings`, payload)
    try {
      await api.put(`/api/v1/domains/${id.value}/waf`, {
        enabled: wafSettings.value.enabled,
        mode: wafSettings.value.mode,
      })
    } catch (e: any) {
      toast.error(e?.data?.error || '高级设置已保存，但 WAF 开关保存失败')
      return
    }
    toast.success('高级设置已保存，边缘约 15 秒内同步')
    await reloadDomain()
    await loadSettings()
  } catch (e: any) {
    toast.error(e?.data?.error || '保存失败')
  } finally {
    submitting.value = false
  }
}

watch(id, loadSettings, { immediate: true })
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">高级设置</div>
        <div class="card-desc">缓存键、回源、签名 URL、协议与 CC 防御</div>
      </div>
      <div class="card-extra">
        <button class="btn btn-primary btn-sm" type="button" :disabled="submitting || pageLoading" @click="saveSettings">
          {{ submitting ? '保存中…' : '保存设置' }}
        </button>
      </div>
    </div>
    <div v-if="pageLoading" class="card-body text-muted">正在加载高级设置…</div>
    <div v-else class="card-body">
<div class="form-col settings-form">
        <div class="alert alert-info">
          <AppIcon name="info" :size="16" class="icon" />
          <span>保存后边缘节点约 15 秒内自动同步，无需停用或重启。</span>
        </div>

        <div v-if="domain?.published_version_id" class="alert alert-warning">
          <AppIcon name="info" :size="16" class="icon" />
          <span>该域名已启用配置发布：保存后不会立即下发，请在「配置发布」创建并审批发布单后才会切换边缘版本。</span>
        </div>

        <!-- 缓存 -->
        <section class="settings-group">
          <h4 class="settings-group-title">缓存</h4>
          <label
            class="switch"
            :class="{ on: settings.cache_enabled !== false, disabled: cacheBusy }"
            @click.prevent="toggleCache"
          >
            <span class="switch-track" />
            启用缓存
          </label>
          <span class="field-hint" style="margin-top: -4px; margin-bottom: 12px">
            关闭后该站点所有请求不读不写缓存，并立即清理已有缓存。默认开启。
          </span>

          <label class="switch" :class="{ on: settings.ignore_query_string }">
            <span class="switch-track" />
            <input v-model="settings.ignore_query_string" type="checkbox" style="display: none" />
            忽略 URL 查询串（缓存键不含 ?query）
          </label>

          <div class="field">
            <label class="field-label">Cache Key 忽略请求头</label>
            <div class="check-row">
              <label class="check">
                <input v-model="settings.ignore_origin" type="checkbox" />
                Origin
              </label>
              <label class="check">
                <input v-model="settings.ignore_accept_language" type="checkbox" />
                Accept-Language
              </label>
            </div>
            <span class="field-hint">
              仅当响应内容不随该请求头变化时使用。忽略 Origin 前须确认 Access-Control-Allow-Origin 固定；
              忽略 Accept-Language 前须确认站点不按语言返回不同内容。规则可覆盖此设置。
            </span>
          </div>
        </section>

        <!-- 访问控制 -->
        <section class="settings-group">
          <h4 class="settings-group-title">访问控制</h4>
          <div class="form-grid">
            <div class="field">
              <label class="field-label">CORS 允许源</label>
              <input v-model="settings.cors_origins" class="input" placeholder="* 或 https://a.com" />
            </div>
            <div class="field">
              <label class="field-label">防盗链 Referer 白名单</label>
              <input v-model="settings.hotlink_allow_referers" class="input"
                placeholder="example.com,*.example.com（留空关闭）" />
            </div>
          </div>
        </section>

        <!-- 回源 -->
        <section class="settings-group">
          <h4 class="settings-group-title">回源</h4>
          <div class="field">
            <label class="field-label">回源鉴权 Header</label>
            <input v-model="settings.origin_auth_header" class="input" placeholder="例如 x-origin-token" />
          </div>
          <div class="field">
            <label class="field-label">回源鉴权值</label>
            <div class="secret-row">
              <StatusTag :tone="secretsConfigured.origin_auth_value ? 'success' : 'neutral'" dot>
                {{ secretsConfigured.origin_auth_value ? '已配置' : '未配置' }}
              </StatusTag>
              <span class="text-muted secret-note">
                {{ secretsConfigured.origin_auth_value
                  ? '值不在页面回显。留空保存即保持原值。'
                  : '填写后 Edge 会把它注入到回源请求的上面这个 Header。' }}
              </span>
              <span class="spacer" />
              <button class="btn btn-text btn-sm" type="button" @click="toggleOriginAuthInput">
                {{ showOriginAuthInput ? '取消' : (secretsConfigured.origin_auth_value ? '替换' : '填写') }}
              </button>
            </div>
            <input v-if="showOriginAuthInput" v-model="settings.origin_auth_value" class="input" type="password"
              autocomplete="off" :placeholder="secretsConfigured.origin_auth_value ? '输入新值以替换' : '仅 Edge 与源站知道'" />
          </div>
          <div class="form-grid">
            <div class="field">
              <label class="field-label">回源连接超时（毫秒）</label>
              <input v-model.number="settings.origin_timeout_ms" class="input" type="number" min="0"
                placeholder="0 = 使用全局默认" />
              <span class="field-hint">
                0 表示继承 Edge 全局默认。
              </span>
            </div>
            <div class="field">
              <label class="field-label">L2 节点</label>
              <!-- Name-only options keep the dropdown compact in the 480px
                   drawer; the full URL is echoed right below the select. -->
              <select v-model="settings.shield_url" class="select shield-select">
                <option value="">关闭（直接回源普通源站）</option>
                <option v-for="node in enabledShieldNodes" :key="node.id" :value="node.url">
                  {{ shieldNodeLabel(node) }}
                </option>
                <option v-if="currentShieldUnregistered" :value="settings.shield_url">
                  当前值（未添加或已停用）
                </option>
              </select>
              <div v-if="settings.shield_url" class="shield-current">
                <span class="mono shield-current-url" :title="settings.shield_url">
                  {{ settings.shield_url }}
                </span>
              </div>
              <span v-if="currentShieldUnregistered" class="field-hint shield-warn">
                当前地址未添加或已停用；保存前请重新选择可用节点，否则会被拒绝。
              </span>
              <span class="field-hint">
                不选择时直接回源普通源站。地址来自「节点管理 → L2 节点」。
              </span>
            </div>
          </div>
        </section>

        <!-- 签名 URL -->
        <section class="settings-group">
          <h4 class="settings-group-title">签名 URL 防护</h4>
          <div class="form-grid">
            <div class="field">
              <label class="field-label">保护范围</label>
              <select v-model="settings.signed_url_mode" class="select">
                <option value="off">关闭</option>
                <option value="all">整个域名都要求签名</option>
                <option value="paths">仅指定路径要求签名</option>
              </select>
            </div>
            <div class="field">
              <label class="field-label">签名有效期上限（秒）</label>
              <input v-model.number="settings.signed_url_ttl_secs" class="input" type="number" min="1" />
            </div>
          </div>
          <span class="field-hint">
            选择「仅指定路径」后，只需保护 /private/*、/downloads/* 等私有目录，公开页面和静态资源不受影响。
          </span>

          <div v-if="settings.signed_url_mode === 'paths'" class="field">
            <label class="field-label">需要签名的路径</label>
            <textarea v-model="settings.signed_url_paths" class="textarea" rows="3"
              placeholder="/private/*&#10;/downloads/*&#10;/vip/exact-file.mp4" />
            <span class="field-hint">每行一个规则，支持精确路径或 /prefix/*；不支持 .. 和正则。</span>
          </div>

          <div class="field">
            <label class="field-label">防盗链签名</label>
            <label class="switch" :class="{ on: settings.hotlink_signature_enabled }">
              <span class="switch-track" />
              <input v-model="settings.hotlink_signature_enabled" type="checkbox" style="display: none" />
              使用签名 URL 密钥绕过 Referer 防盗链
            </label>
            <span class="field-hint">开启后，Referer 不在白名单时必须携带同一套 expires+sig；公开资源不受影响。</span>
          </div>

          <!-- Key state, not a value field: the secret is write-only over the API,
               so the console shows status and offers an explicit rotation. -->
          <div class="field">
            <label class="field-label">当前签名密钥</label>
            <div class="secret-row">
              <StatusTag :tone="secretsConfigured.signed_url_secret ? 'success' : 'neutral'" dot>
                {{ secretsConfigured.signed_url_secret ? '已配置' : '未配置' }}
              </StatusTag>
              <span class="text-muted secret-note">
                {{ secretsConfigured.signed_url_secret
                  ? '密钥不在页面回显。轮换后旧密钥会作为「上一个密钥」继续被接受。'
                  : '尚未生成密钥。可点右侧「生成密钥」，开启防护时也会自动创建。' }}
              </span>
              <span class="spacer" />
              <button class="btn btn-secondary btn-sm" type="button" :disabled="rotatingSecret"
                @click="rotateSignedUrlSecret">
                {{ rotatingSecret
                  ? '处理中…'
                  : (secretsConfigured.signed_url_secret ? '轮换密钥' : '生成密钥') }}
              </button>
            </div>
          </div>

          <div class="field">
            <label class="field-label">上一个签名密钥（轮换兼容）</label>
            <div class="secret-row">
              <StatusTag :tone="secretsConfigured.signed_url_previous_secret ? 'warning' : 'neutral'" dot>
                {{ secretsConfigured.signed_url_previous_secret ? '生效中' : '无' }}
              </StatusTag>
              <span class="text-muted secret-note">
                {{ secretsConfigured.signed_url_previous_secret
                  ? '使用旧密钥签发的链接仍然可用；再次轮换后旧密钥将被替换。'
                  : '当前没有兼容密钥，旧链接已失效。' }}
              </span>
            </div>
          </div>

          <details class="advanced-secret">
            <summary>高级：手动指定密钥</summary>
            <p class="text-muted" style="font-size: 12px; margin: 6px 0 10px">
              仅在你需要把同一把密钥同步到多个域名时使用。手工覆盖密钥会让此前签发的链接<strong>立即失效</strong>；
              日常轮换请使用上面的「轮换密钥」按钮。
            </p>
            <div class="form-grid">
              <div class="field">
                <label class="field-label">当前密钥（留空保持不变）</label>
                <input v-model="settings.signed_url_secret" class="input" type="text" autocomplete="off" />
              </div>
              <div class="field">
                <label class="field-label">上一个密钥（留空保持不变）</label>
                <input v-model="settings.signed_url_previous_secret" class="input" type="text" autocomplete="off" />
              </div>
            </div>
          </details>
        </section>

        <!-- 协议与优化 -->
        <section class="settings-group">
          <h4 class="settings-group-title">协议与优化</h4>
          <p class="field-hint" style="margin-top: 0">
            按站点开关；新站点的 HTTP/3 默认关闭，启用前需确认边缘 UDP 监听和防火墙已配置。图片 WebP 只转换可缓存且源站带 ETag 的 JPEG/PNG，
            只发给 Accept 含 image/webp 的客户端：首次请求返回原图并在后台转码，之后返回 WebP。
            边缘需为含 WebP 的版本（<code>lagran-edge version</code> 显示 image-webp）。
          </p>
          <label class="check">
            <input v-model="settings.websocket_enabled" type="checkbox" />
            WebSocket
          </label>
          <label class="check">
            <input v-model="settings.http2_enabled" type="checkbox" />
            HTTP/2
          </label>
          <label class="check">
            <input v-model="settings.http3_enabled" type="checkbox" />
            HTTP/3（QUIC）
          </label>
          <label class="check">
            <input v-model="settings.gzip_enabled" type="checkbox" />
            Gzip 压缩
          </label>
          <label class="check">
            <input v-model="settings.image_webp_enabled" type="checkbox" />
            图片自动 WebP
          </label>
          <label class="check">
            <input v-model="settings.access_log_enabled" type="checkbox" />
            记录访问日志
          </label>
          <p class="field-hint">关闭后，这个站点的请求不再写入访问日志。边缘同步大约需要 15 秒。</p>
          <div class="form-grid" style="margin-top: 12px">
            <div class="field">
              <label class="field-label">最低 TLS 版本</label>
              <select v-model="settings.min_tls_version" class="select">
                <option value="1.0">TLS 1.0</option>
                <option value="1.1">TLS 1.1</option>
                <option value="1.2">TLS 1.2（推荐）</option>
                <option value="1.3">TLS 1.3</option>
              </select>
              <span class="field-hint">客户端低于此版本无法完成 HTTPS 握手；默认 1.2。</span>
            </div>
            <div class="field">
              <label class="field-label">DNS 接入方式</label>
              <select v-model="settings.access_mode" class="select">
                <option value="cname">CNAME</option>
                <option value="ns">整域托管</option>
              </select>
              <span class="field-hint">
                CNAME：精确主机名 HTTP-01；泛域名需绑定 DNS 服务商。整域托管：平台写 TXT，自动签发根域与泛域名。
              </span>
            </div>
            <div v-if="settings.access_mode === 'cname'" class="field">
              <label class="field-label">DNS 服务商（泛域名证书）</label>
              <select v-model="settings.dns_provider_id" class="select">
                <option value="">不绑定（仅精确主机名可自动签发）</option>
                <option v-for="p in enabledDnsProviders" :key="p.id" :value="p.id">
                  {{ p.name }}
                </option>
              </select>
              <span class="field-hint">
                账号在「智能 DNS → DNS 服务商」维护。绑定后可为站点下的
                <code>*.domain</code> 自动申请证书。
              </span>
            </div>
            <div class="field">
              <label class="field-label">自动 HTTPS</label>
              <label class="check" style="margin-top: 8px">
                <input v-model="settings.auto_https" type="checkbox" />
                接入就绪后自动签发并续签证书
              </label>
              <span class="field-hint">
                证书状态：
                {{
                  settings.cert_status === 'active'
                    ? '已签发'
                    : settings.cert_status === 'pending'
                      ? '签发中 / 等待 DNS'
                      : settings.cert_status === 'error'
                        ? '签发失败'
                        : '未签发'
                }}
              </span>
            </div>
          </div>
        </section>

        <!-- WAF：开关可在此改；细则在左侧「WAF 防护」 -->
        <section class="settings-group">
          <h4 class="settings-group-title">WAF 防护</h4>
          <p class="field-hint" style="margin-top: 0">
            按站点开关。规则、IP、地区与事件请到左侧「WAF 防护」配置。
          </p>
          <div v-if="wafLoading" class="text-muted" style="font-size: 13px">读取 WAF 设置…</div>
          <template v-else>
            <label class="switch" :class="{ on: wafSettings.enabled }">
              <span class="switch-track" />
              <input v-model="wafSettings.enabled" type="checkbox" style="display: none" />
              启用 Web 防火墙
            </label>
            <div class="form-grid" style="margin-top: 12px">
              <div class="field">
                <label class="field-label">防护模式</label>
                <select v-model="wafSettings.mode" class="select" :disabled="!wafSettings.enabled">
                  <option value="monitor">观察（只记录）</option>
                  <option value="block">拦截（正式防护）</option>
                </select>
              </div>
            </div>
            <NuxtLink
              v-if="id"
              class="btn btn-default btn-sm"
              style="margin-top: 10px"
              :to="`/domains/${encodeURIComponent(id)}/waf/overview`"
            >
              打开 WAF 防护
            </NuxtLink>
          </template>
        </section>

        <!-- CC 防御 -->
        <section class="settings-group">
          <h4 class="settings-group-title">CC 防御</h4>
          <label class="check">
            <input v-model="settings.cc_enabled" type="checkbox" />
            启用 CC 防御（按客户端聚合组限速，超限自动挑战/封禁）
          </label>
          <span class="field-hint">
            全局限流 RPS 是域名总阈值：超过后自动进入 60 秒攻击模式，客户端限额减半；HTML 浏览器首次超限
            进入 PoW 挑战，验证通过后获得临时 2x 预算，持续超限才封禁。
          </span>

          <div class="form-grid">
            <div class="field">
              <label class="field-label">全局限流 RPS（0 = 不限）</label>
              <input v-model.number="settings.rate_limit_rps" class="input" type="number" />
            </div>
            <div class="field">
              <label class="field-label">全局限流突发</label>
              <input v-model.number="settings.rate_limit_burst" class="input" type="number" />
            </div>
            <div class="field">
              <label class="field-label">每 IP 每秒请求数</label>
              <input v-model.number="settings.cc_rps" class="input" type="number" min="1" />
            </div>
            <div class="field">
              <label class="field-label">突发额度</label>
              <input v-model.number="settings.cc_burst" class="input" type="number" min="0" />
            </div>
            <div class="field">
              <label class="field-label">连续超限次数后封禁</label>
              <input v-model.number="settings.cc_ban_after" class="input" type="number" min="1" />
            </div>
            <div class="field">
              <label class="field-label">封禁时长（秒）</label>
              <input v-model.number="settings.cc_ban_seconds" class="input" type="number" min="1" max="86400" />
            </div>
            <div class="field">
              <label class="field-label">模式</label>
              <select v-model="settings.cc_mode" class="select">
                <option value="block">拦截（429）</option>
                <option value="monitor">观察（只记录）</option>
              </select>
            </div>
            <div class="field">
              <label class="field-label">IP 白名单</label>
              <input v-model="settings.cc_allow_ips" class="input" placeholder="10.0.0.0/8, 203.0.113.7" />
            </div>
          </div>
          <span class="field-hint">白名单逗号分隔，支持 IPv4/IPv6 与 CIDR；白名单 IP 不受 CC 限制。</span>
        </section>

        <!-- 响应头 -->
        <section class="settings-group">
          <h4 class="settings-group-title">自定义响应头</h4>
          <textarea v-model="settings.custom_headers" class="textarea mono" rows="3"
            placeholder="X-CDN: lagran&#10;X-Frame-Options: DENY" />
          <span class="field-hint">每行一个，格式 Name: Value。</span>
        </section>
      </div>
    </div>
  </div>

  <BaseModal v-model="rotatedSecretOpen" title="新的签名密钥" confirm-text="我已保存" @confirm="closeRotatedSecret"
    @close="closeRotatedSecret">
    <div class="form-col">
      <div class="alert alert-warning" role="alert">
        <AppIcon name="alert" :size="16" class="icon" />
        <span>该密钥只显示这一次，关闭后无法再次查看。请立即复制并妥善保存。</span>
      </div>
      <div class="field">
        <label class="field-label">签名密钥</label>
        <div class="secret-row">
          <input class="input mono" :value="rotatedSecret" readonly />
          <button class="btn btn-secondary btn-sm" type="button" @click="copyRotatedSecret">
            复制
          </button>
        </div>
      </div>
    </div>
  </BaseModal>
</template>

<style scoped>
.settings-form {
  gap: 18px;
}

.settings-group {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg, 10px);
  background: var(--card);
}

.settings-group-title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  letter-spacing: 0.02em;
}

/* A short rule under the title separates it from the first field without
   adding height. */
.settings-group-title::after {
  content: "";
  display: block;
  margin-top: 8px;
  height: 1px;
  background: var(--border);
}

.settings-group .form-grid {
  gap: 12px 14px;
}

.settings-group .field-hint {
  margin-top: -4px;
}

/* Origin Shield selector: options show the node name only, and the full
   address is echoed under the select instead of inside the option. */
.shield-select {
  text-overflow: ellipsis;
}

.shield-current {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  padding: 6px 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: color-mix(in srgb, var(--brand) 3%, var(--card));
}

.shield-current-label {
  flex: none;
  color: var(--text-3);
  font-size: 11px;
  line-height: 1.4;
}

.shield-current-url {
  min-width: 0;
  flex: 1;
  color: var(--text-2);
  font-size: 11.5px;
  line-height: 1.4;
  word-break: break-all;
}

.settings-group .shield-warn {
  margin-top: -2px;
  color: var(--warning);
}

.switch.disabled {
  opacity: 0.45;
  pointer-events: none;
}

/* Inline row of checkboxes, used for cache-key headers. */
.check-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 20px;
}

/* Status row for write-only secrets: badge + explanation + action. */
.secret-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  min-height: 28px;
}

.secret-row .input {
  flex: 1;
  min-width: 0;
}

.secret-note {
  font-size: 12.5px;
  line-height: 1.4;
}

/* Manual key entry is an escape hatch, not the normal path. */
.advanced-secret {
  padding: 10px 12px;
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius);
  background: transparent;
}

.advanced-secret summary {
  cursor: pointer;
  font-size: 12.5px;
  color: var(--text-2);
  user-select: none;
}

.advanced-secret[open] summary {
  margin-bottom: 2px;
}

</style>
