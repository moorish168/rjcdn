import { copyText } from '~/utils/clipboard'
import { friendlyErrorMessage } from '~/utils/friendlyError'
import {
  CACHE_BACKEND_LABEL,
  EDGE_PROFILE_DEFAULTS,
  humanBytes,
  isValidListen,
  isValidSocketAddr,
  parseEdgeProfile,
  recommendEdgeConfig,
  renderEdgeEnvFile,
  serializeEdgeProfile,
  splitListen,
  summarizeHardware,
  type EdgeCacheBackend,
  type EdgeDiskType,
  type EdgeNodeRole,
  type EdgeProfile,
  type EdgeWorkload,
} from '~/utils/edgeSizing'

export interface NodeDetail {
  node: any
  token: string
  control_url?: string
}

function cloneProfile(profile: EdgeProfile): EdgeProfile {
  return {
    hardware: { ...profile.hardware },
    infra: { ...profile.infra },
    sizing: profile.sizing
      ? { ...profile.sizing, itemValues: { ...(profile.sizing.itemValues ?? {}) } }
      : undefined,
    env: profile.env ?? '',
  }
}

export function useNodeDetail(nodeId: Ref<string> | ComputedRef<string>) {
  const api = useApi()
  const toast = useToast()
  const { ask } = useConfirm()

  const detail = ref<NodeDetail | null>(null)
  const notFound = ref(false)
  const loading = ref(false)
  const saving = ref(false)
  const savingProfile = ref(false)
  const upgrading = ref(false)

  const detailProfile = ref<EdgeProfile>(cloneProfile(EDGE_PROFILE_DEFAULTS))
  const detailProfileStored = ref(false)

  const detailPlan = computed(() =>
    recommendEdgeConfig(detailProfile.value.hardware, {
      ...(detailProfile.value.sizing ?? {}),
      cacheBackend: detailProfile.value.infra.cacheBackend,
      redisConfigured: Boolean(
        detailProfile.value.infra.redisUrl.trim() || detailProfile.value.infra.cacheRedisUrl.trim(),
      ),
    }),
  )
  const detailHardwareSummary = computed(() => summarizeHardware(detailProfile.value.hardware))
  const detailHasOverrides = computed(() => {
    const sizing = detailProfile.value.sizing
    return Boolean(
      sizing
      && ((sizing.itemValues && Object.keys(sizing.itemValues).length > 0)
        || sizing.largeCacheGiB !== undefined),
    )
  })

  const detailListenerWarning = computed(() => {
    if (!isValidSocketAddr(detailProfile.value.infra.adminListen.trim())) {
      return 'EDGE_ADMIN_LISTEN 必须是 IP:端口（不能写主机名），例如 0.0.0.0:8081。'
    }
    if (!isValidListen(detailProfile.value.infra.httpListen.trim())) {
      return 'EDGE_LISTEN 格式应为 IP/主机:端口，例如 0.0.0.0:80。'
    }
    if (
      detailProfile.value.infra.httpsEnabled
      && !isValidListen(detailProfile.value.infra.httpsListen.trim())
    ) {
      return 'HTTPS 监听地址格式应为 IP/主机:端口，例如 0.0.0.0:443。'
    }
    return ''
  })

  const detailCallbackWarning = computed(() => {
    const agent = detailProfile.value.infra.agentUrl.trim()
    const admin = splitListen(detailProfile.value.infra.adminListen)
    if (!agent || !admin) return ''
    try {
      const url = new URL(agent)
      const port = url.port || (url.protocol === 'https:' ? '443' : '80')
      const agentHost = url.hostname.replace(/^\[/, '').replace(/\]$/, '').toLowerCase()
      const adminHost = admin.host.replace(/^\[/, '').replace(/\]$/, '').toLowerCase()
      const loopback = (host: string) =>
        host === 'localhost' || host === '::1' || host.startsWith('127.')
      if (port !== admin.port) {
        return `AGENT_URL 端口 ${port} 与 EDGE_ADMIN_LISTEN 端口 ${admin.port} 不一致，purge / 证书 / ACME 回调会失败。`
      }
      if (loopback(adminHost) && !loopback(agentHost)) {
        return '管理面只监听回环地址，但 AGENT_URL 使用非回环地址，控制面无法回调。'
      }
      if (!loopback(adminHost) && loopback(agentHost)) {
        return 'AGENT_URL 指向回环地址，跨机控制面无法回调。'
      }
    } catch {
      return 'AGENT_URL 不是合法的 http(s) 地址。'
    }
    return ''
  })

  const detailEnv = computed(() => {
    if (!detail.value) return ''
    const node = detail.value.node || {}
    return renderEdgeEnvFile({
      nodeId: node.name || '',
      region: node.region || '',
      publicIp: node.public_ip || '',
      agentUrl:
        detailProfile.value.infra.agentUrl.trim()
        || node.agent_url
        || (node.public_ip ? `http://${node.public_ip}:8081` : ''),
      token: detail.value.token || '',
      controlUrl:
        detail.value.control_url || (import.meta.client ? window.location.origin : ''),
      httpListen: detailProfile.value.infra.httpListen,
      adminListen: detailProfile.value.infra.adminListen,
      httpsEnabled: detailProfile.value.infra.httpsEnabled,
      httpsListen: detailProfile.value.infra.httpsListen,
      redisUrl: detailProfile.value.infra.redisUrl,
      cacheRedisUrl: detailProfile.value.infra.cacheRedisUrl,
      challengeSecret: detailProfile.value.infra.challengeSecret,
      wafRulepackSecret: detailProfile.value.infra.wafRulepackSecret,
      hardware: detailProfile.value.hardware,
      plan: detailPlan.value,
    })
  })

  function applyStoredProfile(res: NodeDetail) {
    const stored = parseEdgeProfile((res.node as any)?.edge_profile)
    detailProfile.value = stored ?? cloneProfile(EDGE_PROFILE_DEFAULTS)
    detailProfileStored.value = stored !== null
    if (!detailProfile.value.infra.agentUrl) {
      detailProfile.value.infra.agentUrl = (res.node as any)?.agent_url || ''
    }
  }

  async function load() {
    if (!nodeId.value) return
    loading.value = true
    notFound.value = false
    try {
      const res = await api.get<NodeDetail>(`/api/v1/nodes/${nodeId.value}`)
      detail.value = res
      applyStoredProfile(res)
    } catch {
      notFound.value = true
      detail.value = null
    } finally {
      loading.value = false
    }
  }

  function setDetailDiskType(value: string) {
    detailProfile.value.hardware.diskType = value as EdgeDiskType
  }
  function setDetailWorkload(value: string) {
    detailProfile.value.hardware.workload = value as EdgeWorkload
  }
  function setDetailRole(value: string) {
    detailProfile.value.hardware.role = value as EdgeNodeRole
  }
  function setDetailCacheBackend(value: string) {
    detailProfile.value.infra.cacheBackend = value as EdgeCacheBackend
  }

  async function saveDetailProfile() {
    const node = detail.value?.node
    if (!node?.id) return
    const problem = detailListenerWarning.value || detailCallbackWarning.value
    if (problem) {
      toast.error(problem)
      return
    }
    savingProfile.value = true
    try {
      await api.put(`/api/v1/nodes/${node.id}`, {
        agent_url: detailProfile.value.infra.agentUrl.trim(),
        max_bandwidth_mbps: Math.round(Number(detailProfile.value.hardware.bandwidthMbps) || 0),
        edge_profile: serializeEdgeProfile({
          ...detailProfile.value,
          env: detailEnv.value,
        }),
      })
      detailProfileStored.value = true
      toast.success('硬件画像已保存，配置已按新画像重算')
      await load()
    } catch (e: any) {
      toast.error(e?.data?.error || '保存硬件画像失败')
    } finally {
      savingProfile.value = false
    }
  }

  function clearDetailOverrides() {
    detailProfile.value.sizing = {}
    void saveDetailProfile()
  }

  function downloadDetailEnv() {
    if (!import.meta.client || !detailEnv.value) return
    const name = detail.value?.node?.name || 'edge'
    const blob = new Blob([detailEnv.value], { type: 'text/plain;charset=utf-8' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `edge-${name}.env`
    a.click()
    URL.revokeObjectURL(url)
  }

  async function copy(text: string, label: string) {
    if (await copyText(text)) toast.success(`${label}已复制`)
    else toast.error('复制失败，请手动选择文本')
  }

  async function toggle() {
    const node = detail.value?.node
    if (!node) return
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

  async function upgrade() {
    const node = detail.value?.node
    if (!node) return
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
    upgrading.value = true
    try {
      const result = await api.post<any>(`/api/v1/nodes/${node.id}/upgrade`, {})
      toast.success(`升级任务已下发：${result.version}，节点将在下载校验后自动重启`)
      window.setTimeout(() => void load(), 5000)
      window.setTimeout(() => void load(), 15000)
    } catch (e: any) {
      toast.error(friendlyErrorMessage(e?.data?.error, '升级任务下发失败'))
    } finally {
      window.setTimeout(() => {
        upgrading.value = false
      }, 20000)
    }
  }

  async function remove() {
    const node = detail.value?.node
    if (!node) return
    const ok = await ask({
      title: '删除节点',
      message: `删除 ${node.name} 后会吊销其令牌并停止下发刷新/证书。如果该节点仍在运行，下次心跳会自动重新注册。`,
      confirmText: '删除',
      danger: true,
    })
    if (!ok) return
    try {
      await api.del(`/api/v1/nodes/${node.id}`)
      toast.success('节点已删除')
      await navigateTo('/nodes')
    } catch (e: any) {
      toast.error(e?.data?.error || '删除失败')
    }
  }

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

  return {
    CACHE_BACKEND_LABEL,
    humanBytes,
    detail,
    notFound,
    loading,
    saving,
    savingProfile,
    upgrading,
    detailProfile,
    detailProfileStored,
    detailPlan,
    detailHardwareSummary,
    detailHasOverrides,
    detailListenerWarning,
    detailCallbackWarning,
    detailEnv,
    load,
    setDetailDiskType,
    setDetailWorkload,
    setDetailRole,
    setDetailCacheBackend,
    saveDetailProfile,
    clearDetailOverrides,
    downloadDetailEnv,
    copy,
    toggle,
    upgrade,
    remove,
    statusLabel,
    statusTone,
  }
}
