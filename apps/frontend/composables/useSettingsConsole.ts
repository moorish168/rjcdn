import { copyText } from '~/utils/clipboard'

export function useSettingsConsole() {

const api = useApi()
const auth = useAuthStore()
const toast = useToast()
const { ask } = useConfirm()
const route = useRoute()
const section = computed(() => (route.path.includes('/settings/advanced') ? 'advanced' : 'basic'))
const brand = useBrand()
const brandNameDraft = ref(brand.systemName.value)
const updateRepoDraft = ref('')
/** Saved value from the server; draft edits do not enable updates until saved. */
const updateRepoSaved = ref('')
/** Repository the root updater of this host uses (self-upgrade). */
const hostRepo = ref('')
/** Repository remote installs and the node wizard use (public default when unset). */
const DEFAULT_UPDATE_REPO = 'moorish168/rjcdn'
const effectiveRepo = ref(DEFAULT_UPDATE_REPO)
const repoPassword = ref('')
/** Draft for a new release token; empty means leave unchanged on save. */
const updateTokenDraft = ref('')
/** Whether the server already has a token (DB or env). */
const updateTokenConfigured = ref(false)
/** Clear the stored console token on next save. */
const clearUpdateToken = ref(false)
const brandSaving = ref(false)
/** Update UI is always available with the public default repository. */
const updateRepoConfigured = computed(() => true)
const repoChanged = computed(() => updateRepoDraft.value.trim() !== updateRepoSaved.value.trim())
const tokenChanged = computed(
  () => clearUpdateToken.value || !!updateTokenDraft.value.trim(),
)
const credentialsChanged = computed(() => repoChanged.value || tokenChanged.value)

interface BrandingResponse {
  system_name?: string
  update_repo?: string
  effective_update_repo?: string
  host_update_repo?: string
  update_token_configured?: boolean
}

function applyBranding(cfg: BrandingResponse | null | undefined) {
  const repo = (cfg?.update_repo || '').trim()
  updateRepoDraft.value = repo
  updateRepoSaved.value = repo
  hostRepo.value = (cfg?.host_update_repo || '').trim()
  effectiveRepo.value = (
    cfg?.effective_update_repo ||
    repo ||
    hostRepo.value ||
    DEFAULT_UPDATE_REPO
  ).trim()
  updateTokenConfigured.value = !!cfg?.update_token_configured
  updateTokenDraft.value = ''
  clearUpdateToken.value = false
}

async function loadBrandSettings() {
  if (!auth.isAdmin) return
  try {
    const cfg = await api.get<BrandingResponse>('/api/v1/system/branding')
    if (cfg?.system_name?.trim()) {
      brandNameDraft.value = cfg.system_name.trim()
      brand.systemName.value = cfg.system_name.trim()
    }
    applyBranding(cfg)
  } catch {
    // Keep drafts; update section stays disabled until credentials are known.
  }
}

async function saveBrandSettings() {
  brandSaving.value = true
  try {
    if (credentialsChanged.value && !repoPassword.value) {
      toast.error('修改升级仓库或访问令牌需要输入当前登录密码')
      return
    }
    const payload: Record<string, string | undefined> = {
      system_name: brandNameDraft.value.trim(),
      update_repo: updateRepoDraft.value.trim(),
      current_password: credentialsChanged.value ? repoPassword.value : undefined,
    }
    if (clearUpdateToken.value) {
      payload.update_token = ''
    } else if (updateTokenDraft.value.trim()) {
      payload.update_token = updateTokenDraft.value.trim()
    }
    const body = await api.put<BrandingResponse>('/api/v1/system/branding', payload)
    brandNameDraft.value = (body?.system_name || brandNameDraft.value).trim()
    brand.systemName.value = brandNameDraft.value || '控制台'
    applyBranding(body)
    repoPassword.value = ''
    toast.success('设置已保存')
  } catch (e: any) {
    toast.error(e?.data?.error || '保存失败')
  } finally {
    brandSaving.value = false
  }
}

const config = useRuntimeConfig()
const apiBase = config.public.apiBase as string
const effectiveApiBase = computed(() => {
  const configured = (apiBase || '').trim().replace(/\/+$/, '')
  if (configured) return configured
  return import.meta.client ? window.location.origin : ''
})

const checking = ref(false)
const healthStatus = ref<{ ok: boolean; at: string; message?: string } | null>(null)
const controlVersion = ref('')

interface UpdateInfo {
  current_version: string
  latest_version: string | null
  update_available: boolean
  release_url: string | null
  published_at: string | null
  notes: string | null
  updater_available: boolean
  error: string | null
}
const updateInfo = ref<UpdateInfo | null>(null)
// The release URL comes from the update API; only follow http(s) links so a
// compromised/misconfigured response cannot inject `javascript:` into href.
const releaseLink = computed(() => {
  const raw = updateInfo.value?.release_url
  if (!raw) return null
  try {
    const url = new URL(raw)
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : null
  } catch {
    return null
  }
})
const updateChecking = ref(false)
const updateApplying = ref(false)
const UPDATE_PENDING_KEY = 'lagran-update-pending'
try {
  updateApplying.value = window.localStorage.getItem(UPDATE_PENDING_KEY) === '1'
} catch {
  // localStorage can be unavailable in privacy mode; server status still wins.
}
const updateMessage = ref('')
const updateStatusInfo = ref<{ state: string; message: string | null } | null>(null)
const ACTIVE_UPDATE_STATES = new Set([
  'queued',
  'pending',
  'downloading',
  'installing',
  'restarting',
  'running',
])
const updateBusy = computed(
  () =>
    updateApplying.value ||
    ACTIVE_UPDATE_STATES.has(String(updateStatusInfo.value?.state || '').toLowerCase()),
)
let updatePollTimer: number | null = null

function clearUpdatePoll() {
  if (updatePollTimer != null) {
    window.clearTimeout(updatePollTimer)
    updatePollTimer = null
  }
}

function scheduleUpdatePoll(delay = 5000) {
  clearUpdatePoll()
  updatePollTimer = window.setTimeout(async () => {
    await loadUpdateStatus()
    if (updateBusy.value) scheduleUpdatePoll()
  }, delay)
}

async function checkUpdate() {
  if (!auth.isAdmin || !updateRepoConfigured.value) return
  updateChecking.value = true
  updateMessage.value = ''
  try {
    const res = await api.get<UpdateInfo>('/api/v1/system/update')
    updateInfo.value = res
    if (res.current_version) controlVersion.value = res.current_version
    if (res.error) updateMessage.value = res.error
  } catch (e: any) {
    updateMessage.value = e?.data?.error || e?.message || '检测更新失败'
  } finally {
    updateChecking.value = false
  }
}

async function loadUpdateStatus() {
  if (!auth.isAdmin) return
  try {
    const res = await api.get<{ state: string; message: string | null }>(
      '/api/v1/system/update/status',
    )
    updateStatusInfo.value = res
    const state = String(res.state || '').toLowerCase()
    if (ACTIVE_UPDATE_STATES.has(state)) {
      updateApplying.value = true
      scheduleUpdatePoll()
    } else if (state === 'success') {
      updateApplying.value = false
      try {
        window.localStorage.removeItem(UPDATE_PENDING_KEY)
      } catch {}
      clearUpdatePoll()
      // Refresh current/latest versions so the now-obsolete update button is
      // removed immediately after the control plane comes back.
      checkUpdate()
    } else if (state === 'failed') {
      updateApplying.value = false
      try {
        window.localStorage.removeItem(UPDATE_PENDING_KEY)
      } catch {}
      clearUpdatePoll()
    }
  } catch {
    // The control plane restarts during an update; keep the button disabled
    // and retry until the status endpoint is reachable again.
    if (updateApplying.value) scheduleUpdatePoll()
  }
}

async function applyUpdate() {
  if (!updateRepoConfigured.value) return
  if (!updateInfo.value?.latest_version) return
  if (updateBusy.value) return
  const ok = await ask({
    title: '确认系统更新',
    message: `确定更新到 ${updateInfo.value.latest_version}？控制面会短暂重启，请确认当前无重要操作。`,
    confirmText: '开始更新',
    danger: true,
  })
  if (!ok) return
  updateApplying.value = true
  try {
    window.localStorage.setItem(UPDATE_PENDING_KEY, '1')
  } catch {}
  updateMessage.value = '更新任务已提交，正在下载并校验 Release…'
  try {
    const res = await api.post<{ queued: boolean; version: string; message: string }>(
      '/api/v1/system/update',
      { version: updateInfo.value.latest_version },
    )
    updateMessage.value = res.message || '更新任务已排队'
    toast.success('更新任务已提交')
    scheduleUpdatePoll(3000)
  } catch (e: any) {
    updateMessage.value = e?.data?.error || e?.message || '提交更新失败'
    // A 409 from the backend means another update is already running. Re-read
    // status so the button stays disabled until it finishes.
    const state = String(e?.data?.state || '').toLowerCase()
    if (
      e?.status === 409 ||
      e?.statusCode === 409 ||
      e?.response?.status === 409 ||
      ACTIVE_UPDATE_STATES.has(state)
    ) {
      updateApplying.value = true
      try {
        window.localStorage.setItem(UPDATE_PENDING_KEY, '1')
      } catch {}
      await loadUpdateStatus()
      scheduleUpdatePoll()
    } else {
      updateApplying.value = false
      try {
        window.localStorage.removeItem(UPDATE_PENDING_KEY)
      } catch {}
      clearUpdatePoll()
    }
    toast.error(updateMessage.value)
  }
}

const profileOpen = ref(false)
const profileName = ref('')
const profileError = ref('')
const passwordOpen = ref(false)
const currentPassword = ref('')
const newPassword = ref('')
const passwordError = ref('')
const saving = ref(false)
const emailOpen = ref(false)
const newEmail = ref('')
const emailPassword = ref('')
const emailError = ref('')

function openEmail() {
  newEmail.value = ''
  emailPassword.value = ''
  emailError.value = ''
  emailOpen.value = true
}

async function saveEmail() {
  emailError.value = ''
  const email = newEmail.value.trim()
  if (!email || !emailPassword.value) {
    emailError.value = '请填写新账号和当前密码'
    return
  }
  saving.value = true
  try {
    const res = await api.put<{ success: boolean; token: string }>('/api/v1/auth/me/email', {
      email,
      current_password: emailPassword.value,
    })
    if (res.token && auth.user) {
      // Changing the e-mail revokes the old JWT; continue with the new one.
      auth.setSession(res.token, { ...auth.user, email: email.toLowerCase() })
    }
    emailOpen.value = false
    toast.success('登录账号已修改，下次请用新账号登录')
  } catch (e: any) {
    emailError.value = e?.data?.error || '修改失败'
  } finally {
    saving.value = false
  }
}

function openProfile() {
  profileName.value = auth.user?.name || ''
  profileError.value = ''
  profileOpen.value = true
}

function openPassword() {
  currentPassword.value = ''
  newPassword.value = ''
  passwordError.value = ''
  passwordOpen.value = true
}

async function saveProfile() {
  profileError.value = ''
  saving.value = true
  try {
    const user = await api.put<any>('/api/v1/auth/me', {
      name: profileName.value.trim(),
    })
    auth.setSession(auth.token!, user)
    profileOpen.value = false
    toast.success('资料已更新')
  } catch (e: any) {
    profileError.value = e?.data?.error || '保存失败'
  } finally {
    saving.value = false
  }
}

async function savePassword() {
  passwordError.value = ''
  saving.value = true
  try {
    const res = await api.put<{ success: boolean; token: string }>(
      '/api/v1/auth/me/password',
      {
        current_password: currentPassword.value,
        new_password: newPassword.value,
      },
    )
    if (res.token && auth.user) {
      // The old JWT is revoked by the token-version bump. Keep the current
      // session alive with the newly issued token.
      auth.setSession(res.token, auth.user)
    }
    passwordOpen.value = false
    currentPassword.value = ''
    newPassword.value = ''
    toast.success('密码已修改')
  } catch (e: any) {
    passwordError.value = e?.data?.error || '修改失败'
  } finally {
    saving.value = false
  }
}

const initial = computed(() => (auth.user?.name || auth.user?.email || 'L').slice(0, 1).toUpperCase())

async function copy(text: string, label: string) {
  if (await copyText(text)) {
    toast.success(`${label}已复制`)
  } else {
    toast.error('复制失败，请手动选择文本')
  }
}

async function checkHealth() {
  checking.value = true
  try {
    const res = await fetch(`${effectiveApiBase.value}/health`)
    const text = await res.text()
    if (res.ok) {
      try {
        const data = JSON.parse(text)
        if (data.version) controlVersion.value = data.version
      } catch {
        // Health may be served by a non-JSON proxy; keep the version blank.
      }
    }
    healthStatus.value = {
      ok: res.ok,
      at: new Date().toLocaleTimeString('zh-CN', { hour12: false }),
      message: res.ok ? undefined : `HTTP ${res.status} ${text.slice(0, 80)}`,
    }
  } catch (e: any) {
    healthStatus.value = {
      ok: false,
      at: new Date().toLocaleTimeString('zh-CN', { hour12: false }),
      message: e?.message || 'network error',
    }
  } finally {
    checking.value = false
  }
}

const geoipInfo = ref<{
  path: string
  loaded: boolean
  size_bytes: number
  sha256: string
  updated_at: string | null
  source: string
  mirror_url: string
} | null>(null)
const geoipLoading = ref(false)
const geoipUpdating = ref(false)

async function loadGeoip() {
  if (!auth.isAdmin) return
  geoipLoading.value = true
  try {
    geoipInfo.value = await api.get('/api/v1/system/geoip')
  } catch (e: any) {
    geoipInfo.value = null
    toast.error(e?.data?.error || e?.message || '加载 IP 库状态失败')
  } finally {
    geoipLoading.value = false
  }
}

async function updateGeoip() {
  if (!auth.isAdmin || geoipUpdating.value) return
  geoipUpdating.value = true
  try {
    geoipInfo.value = await api.post('/api/v1/system/geoip')
    toast.success('IP 库已更新，并已通知边缘节点拉取')
  } catch (e: any) {
    toast.error(e?.data?.error || e?.message || '更新 IP 库失败')
  } finally {
    geoipUpdating.value = false
  }
}

function formatBytes(n: number | undefined | null) {
  if (!n || n <= 0) return '—'
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / (1024 * 1024)).toFixed(2)} MB`
}

onMounted(async () => {
  auth.hydrate()
  await brand.refresh()
  brandNameDraft.value = brand.systemName.value
  if (auth.isAdmin) {
    await loadBrandSettings()
    loadGeoip()
    if (updateRepoConfigured.value) {
      checkUpdate()
      loadUpdateStatus()
    }
  }
  fetch(`${effectiveApiBase.value}/health`)
    .then((res) => (res.ok ? res.json() : null))
    .then((data) => {
      if (data?.version) controlVersion.value = data.version
    })
    .catch(() => {})
})

onUnmounted(clearUpdatePoll)

  return {
    applyBranding,
    loadBrandSettings,
    saveBrandSettings,
    clearUpdatePoll,
    scheduleUpdatePoll,
    checkUpdate,
    loadUpdateStatus,
    applyUpdate,
    openEmail,
    saveEmail,
    openProfile,
    openPassword,
    saveProfile,
    savePassword,
    copy,
    checkHealth,
    api,
    auth,
    toast,
    brand,
    brandNameDraft,
    updateRepoDraft,
    updateRepoSaved,
    hostRepo,
    effectiveRepo,
    repoPassword,
    updateTokenDraft,
    updateTokenConfigured,
    clearUpdateToken,
    brandSaving,
    updateRepoConfigured,
    repoChanged,
    tokenChanged,
    credentialsChanged,
    config,
    apiBase,
    effectiveApiBase,
    checking,
    healthStatus,
    controlVersion,
    updateInfo,
    releaseLink,
    updateChecking,
    updateApplying,
    UPDATE_PENDING_KEY,
    updateMessage,
    updateStatusInfo,
    ACTIVE_UPDATE_STATES,
    updateBusy,
    geoipInfo,
    geoipLoading,
    geoipUpdating,
    loadGeoip,
    updateGeoip,
    formatBytes,
    profileOpen,
    profileName,
    profileError,
    passwordOpen,
    currentPassword,
    newPassword,
    passwordError,
    saving,
    emailOpen,
    newEmail,
    emailPassword,
    emailError,
    initial,
    section,
    route,
  }
}
