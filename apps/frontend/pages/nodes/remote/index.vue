<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">远程安装</div>
        <div class="page-desc">
          为边缘节点（含 L2 角色）配置 SSH 并远程安装；SSH 凭证全局通用，DNS 节点在「智能 DNS」里用同一套凭证
        </div>
      </div>
      <div v-if="auth.isAdmin" class="page-actions">
        <NuxtLink to="/nodes" class="btn btn-default">
          <AppIcon name="chevron-left" :size="15" />
          边缘节点
        </NuxtLink>
        <NuxtLink to="/ssh-grants" class="btn btn-default">
          <AppIcon name="shield" :size="15" />
          管理 SSH 凭证
        </NuxtLink>
        <button class="btn btn-default" type="button" :disabled="loading" @click="loadAll">
          <AppIcon name="refresh" :size="15" />
          刷新
        </button>
        <button class="btn btn-primary" type="button" @click="openBatch">
          <AppIcon name="plus" :size="15" />
          批量添加并安装
        </button>
      </div>
    </div>

    <div v-if="!auth.isAdmin" class="alert alert-warning">
      <AppIcon name="shield" :size="16" class="icon" />
      <span>只有管理员可以远程管理节点。</span>
    </div>

    <template v-else>
      <div v-if="!grants.length && !loading" class="alert alert-info">
        <AppIcon name="info" :size="16" class="icon" />
        <span>
          还没有 SSH 凭证。先
          <NuxtLink to="/ssh-grants">添加一组服务器账号</NuxtLink>
          ，再给节点填 SSH 并安装。
        </span>
      </div>

      <div class="card">
        <div class="card-head">
          <div class="card-title">节点</div>
          <div class="flex gap-8" style="flex-wrap: wrap">
            <span class="text-muted" style="font-size: 13px; align-self: center">
              已选 {{ selected.length }} 个
            </span>
            <button
              v-for="a in ACTIONS"
              :key="a.key"
              class="btn btn-sm"
              :class="a.danger ? 'btn-danger' : 'btn-default'"
              type="button"
              :disabled="!selected.length || busy"
              @click="runAction(a.key, selected)"
            >
              批量{{ a.label }}
            </button>
          </div>
        </div>
        <div class="card-body">
          <div v-if="loading && !nodes.length" class="skeleton" style="height: 200px" />
          <div v-else-if="nodes.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th style="width: 36px">
                    <input type="checkbox" :checked="allSelected" @change="toggleAll" />
                  </th>
                  <th>名称</th>
                  <th>状态</th>
                  <th>SSH</th>
                  <th>认证</th>
                  <th>版本</th>
                  <th>最近任务</th>
                  <th style="width: 230px">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="n in nodes" :key="n.id">
                  <td>
                    <input v-model="selected" type="checkbox" :value="n.id" :disabled="!n.ssh_host" />
                  </td>
                  <td>
                    <div>{{ n.name }}</div>
                    <div class="text-muted mono" style="font-size: 12px">{{ n.public_ip || '—' }}</div>
                  </td>
                  <td><StatusTag :status="n.status" dot>{{ statusLabel(n.status) }}</StatusTag></td>
                  <td class="mono">
                    <template v-if="n.ssh_host">{{ n.ssh_host }}:{{ n.ssh_port }}</template>
                    <span v-else class="text-muted">未配置</span>
                  </td>
                  <td>{{ n.grant_name || '—' }}</td>
                  <td class="mono">{{ displayVersion(n.version) || '—' }}</td>
                  <td>
                    <button
                      v-if="n.last_op"
                      class="btn btn-text btn-sm"
                      type="button"
                      :title="n.last_op.error"
                      @click="openLog(n.last_op.id)"
                    >
                      {{ actionLabel(n.last_op.action) }}
                      <StatusTag :tone="opTone(n.last_op.status)">{{ opStatusLabel(n.last_op.status) }}</StatusTag>
                    </button>
                    <span v-else class="text-muted">—</span>
                  </td>
                  <td>
                    <div class="flex gap-8">
                      <button class="btn btn-text btn-sm" type="button" @click="openSsh(n)">SSH 设置</button>
                      <select
                        class="select"
                        style="height: 28px; font-size: 13px; width: 108px"
                        :disabled="!n.ssh_host || busy"
                        value=""
                        @change="onRowAction(n, $event)"
                      >
                        <option value="" disabled>远程操作</option>
                        <option v-for="a in ACTIONS" :key="a.key" :value="a.key">{{ a.label }}</option>
                      </select>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <EmptyState
            v-else
            icon="node"
            title="还没有可远程安装的节点"
            text="先到「边缘节点 / 添加节点」添加，或点右上角「批量添加并安装」。"
          />
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <div class="card-title">任务记录</div>
          <span class="text-muted" style="font-size: 13px">最近 50 条，点击查看日志</span>
        </div>
        <div class="card-body">
          <div v-if="ops.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>时间</th>
                  <th>节点</th>
                  <th>操作</th>
                  <th>状态</th>
                  <th>结果</th>
                  <th>操作人</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="o in ops" :key="o.id" style="cursor: pointer" @click="openLog(o.id)">
                  <td class="mono" style="font-size: 12px">{{ formatTime(o.created_at) }}</td>
                  <td>{{ o.node_name }}</td>
                  <td>{{ actionLabel(o.action) }}{{ o.params?.version ? ` ${displayVersion(o.params.version)}` : '' }}</td>
                  <td><StatusTag :tone="opTone(o.status)">{{ opStatusLabel(o.status) }}</StatusTag></td>
                  <td class="text-muted" style="max-width: 360px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis">
                    {{ o.error || '—' }}
                  </td>
                  <td class="text-muted">{{ o.created_by }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else class="text-muted" style="font-size: 13px">暂无任务</div>
        </div>
      </div>
    </template>

    <!-- Add nodes (single or batch) -->
    <BaseModal
      v-model="batchOpen"
      title="添加节点"
      width="640px"
      :confirm-text="batchResults.length ? '完成' : '添加'"
      :loading="batchSubmitting"
      @confirm="batchResults.length ? closeBatch() : submitBatch()"
    >
      <div v-if="!batchResults.length" class="form-col">
        <div class="field">
          <label class="field-label">SSH 凭证 <span class="req">*</span></label>
          <select v-model="batch.grant_id" class="select">
            <option value="" disabled>请选择</option>
            <option v-for="g in grants" :key="g.id" :value="g.id">{{ g.name }}（{{ g.username }}）</option>
          </select>
        </div>
        <div class="flex gap-8">
          <div class="field" style="flex: 1">
            <label class="field-label">地区标签</label>
            <input v-model="batch.region" class="input" placeholder="例如 hk" />
          </div>
          <div class="field" style="width: 120px">
            <label class="field-label">默认 SSH 端口</label>
            <input v-model.number="batch.port" class="input mono" type="number" min="1" max="65535" />
          </div>
        </div>
        <div class="field">
          <label class="field-label">节点列表 <span class="req">*</span></label>
          <textarea
            v-model="batch.lines"
            class="textarea mono"
            rows="7"
            spellcheck="false"
            placeholder="每行一个：名称 SSH地址[:端口] [公网IP]&#10;hk-01 203.0.113.21&#10;hk-02 203.0.113.22:2222&#10;sg-01 10.0.0.5 198.51.100.7"
          />
          <span class="field-hint">
            SSH 地址是 IP 时默认也作为公网 IP（智能解析 调度用）；内网地址请在第三列写公网 IP。
          </span>
        </div>
        <label class="check-row">
          <input v-model="batch.install" type="checkbox" />
          <span>添加后立即远程安装</span>
        </label>
        <p v-if="batchError" class="text-danger" style="font-size: 13px">{{ batchError }}</p>
      </div>
      <div v-else class="form-col">
        <div v-for="r in batchResults" :key="r.name" class="flex-between" style="font-size: 13px">
          <span>{{ r.name }}</span>
          <span :class="r.ok && !r.error ? 'text-success' : 'text-danger'">
            {{ r.ok ? (r.error ? `已添加，安装未开始：${r.error}` : r.op_id ? '已添加，安装中' : '已添加') : r.error }}
          </span>
        </div>
      </div>
    </BaseModal>

    <!-- SSH settings of one node -->
    <BaseModal
      v-model="sshOpen"
      :title="`SSH 设置：${sshNode?.name || ''}`"
      confirm-text="保存"
      :loading="sshSaving"
      @confirm="saveSsh"
    >
      <div class="form-col">
        <div class="flex gap-8">
          <div class="field" style="flex: 1">
            <label class="field-label">SSH 地址 <span class="req">*</span></label>
            <input v-model="sshForm.host" class="input mono" placeholder="203.0.113.21" />
          </div>
          <div class="field" style="width: 110px">
            <label class="field-label">端口</label>
            <input v-model.number="sshForm.port" class="input mono" type="number" min="1" max="65535" />
          </div>
        </div>
        <div class="field">
          <label class="field-label">SSH 凭证 <span class="req">*</span></label>
          <select v-model="sshForm.grant_id" class="select">
            <option value="" disabled>请选择</option>
            <option v-for="g in grants" :key="g.id" :value="g.id">{{ g.name }}（{{ g.username }}）</option>
          </select>
        </div>
        <div v-if="sshNode?.host_key" class="field">
          <label class="field-label">已记录的主机指纹</label>
          <div class="flex gap-8" style="align-items: center">
            <span class="mono" style="font-size: 12px; word-break: break-all">{{ sshNode.host_key }}</span>
            <button class="btn btn-text btn-sm" type="button" @click="resetKey">重置</button>
          </div>
          <span class="field-hint">服务器重装系统后指纹会变，需要重置才能再次连接。</span>
        </div>
        <p v-if="sshError" class="text-danger" style="font-size: 13px">{{ sshError }}</p>
      </div>
    </BaseModal>

    <!-- Upgrade / install version -->
    <BaseModal
      v-model="versionOpen"
      :title="`${actionLabel(pendingAction)}（${pendingIds.length} 个节点）`"
      confirm-text="开始"
      @confirm="confirmVersion"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">版本</label>
          <input v-model="versionInput" class="input mono" placeholder="留空 = 最新版本，例如 1.0.6" />
          <span class="field-hint">批量升级时所有节点使用同一个版本。</span>
        </div>
      </div>
    </BaseModal>

    <!-- Operation log -->
    <BaseDrawer v-model="logOpen" :title="logTitle" width="720px">
      <div v-if="logOp" class="form-col">
        <div class="flex gap-8" style="align-items: center; flex-wrap: wrap">
          <StatusTag :tone="opTone(logOp.status)">{{ opStatusLabel(logOp.status) }}</StatusTag>
          <span class="text-muted" style="font-size: 13px">
            {{ formatTime(logOp.created_at) }} · {{ logOp.created_by }}
          </span>
        </div>
        <div v-if="logOp.error" class="alert alert-danger">
          <span>{{ logOp.error }}</span>
        </div>
        <pre ref="logBox" class="mono op-log">{{ logOp.log || '等待执行…' }}</pre>
      </div>
      <template #footer>
        <button class="btn btn-default" type="button" @click="logOpen = false">关闭</button>
      </template>
    </BaseDrawer>
  </div>
</template>

<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'
import { displayVersion } from '~/utils/version'

const api = useApi()
const auth = useAuthStore()
const toast = useToast()
const { ask } = useConfirm()

interface Grant { id: string; name: string; username: string }
interface OpSummary { id: string; action: string; status: string; error: string; created_at: string }
interface RemoteNode {
  id: string
  name: string
  region: string
  status: string
  public_ip: string
  version: string
  ssh_host: string
  ssh_port: number
  grant_id: string | null
  grant_name: string
  host_key: string
  last_op: OpSummary | null
}
interface Op {
  id: string
  node_id: string
  node_name: string
  action: string
  status: string
  error: string
  created_by: string
  created_at: string
  log?: string
  params?: { version?: string | null }
}
interface BatchResult { name: string; ok: boolean; node_id?: string; op_id?: string; error?: string }
interface OpResult { node_id: string; op_id?: string; error?: string }

const ACTIONS = [
  { key: 'install', label: '安装', danger: false },
  { key: 'upgrade', label: '升级', danger: false },
  { key: 'reinstall', label: '重装', danger: true },
  { key: 'start', label: '启动', danger: false },
  { key: 'stop', label: '停止', danger: true },
  { key: 'uninstall', label: '卸载', danger: true },
] as const

const nodes = ref<RemoteNode[]>([])
const grants = ref<Grant[]>([])
const ops = ref<Op[]>([])
const loading = ref(false)
const busy = ref(false)
const selected = ref<string[]>([])

function errText(e: any, fallback: string) {
  return friendlyErrorMessage(e?.data?.error || e?.message || e, fallback)
}

function actionLabel(action: string) {
  return ACTIONS.find(a => a.key === action)?.label || action
}

function statusLabel(status: string) {
  return ({ online: '在线', offline: '离线', pending: '待接入', disabled: '已停用' } as Record<string, string>)[status] || status
}

function opStatusLabel(status: string) {
  return ({ queued: '排队中', running: '执行中', success: '成功', failed: '失败' } as Record<string, string>)[status] || status
}

function opTone(status: string) {
  return ({ queued: 'neutral', running: 'info', success: 'success', failed: 'danger' } as Record<string, any>)[status] || 'neutral'
}

function formatTime(raw: string) {
  const d = new Date(raw)
  return Number.isNaN(d.getTime()) ? raw : d.toLocaleString()
}

const selectable = computed(() => nodes.value.filter(n => n.ssh_host).map(n => n.id))
const allSelected = computed(() => selectable.value.length > 0 && selectable.value.every(id => selected.value.includes(id)))

function toggleAll() {
  selected.value = allSelected.value ? [] : [...selectable.value]
}

async function loadAll() {
  if (!auth.isAdmin) return
  loading.value = true
  try {
    const [n, g, o] = await Promise.all([
      api.get<RemoteNode[]>('/api/v1/remote/nodes'),
      api.get<Grant[]>('/api/v1/remote/grants'),
      api.get<Op[]>('/api/v1/remote/ops?limit=50'),
    ])
    nodes.value = n
    grants.value = g
    ops.value = o
    selected.value = selected.value.filter(id => n.some(x => x.id === id))
  } catch (e: any) {
    toast.error(errText(e, '读取节点失败'))
  } finally {
    loading.value = false
  }
}

// ---- actions ----
const versionOpen = ref(false)
const versionInput = ref('')
const pendingAction = ref('')
const pendingIds = ref<string[]>([])

async function runAction(action: string, ids: string[]) {
  if (!ids.length) return
  const label = actionLabel(action)
  const names = nodes.value.filter(n => ids.includes(n.id)).map(n => n.name)
  const preview = names.slice(0, 5).join('、') + (names.length > 5 ? ` 等 ${names.length} 个` : '')
  if (action === 'reinstall' || action === 'uninstall' || action === 'stop') {
    const warn = {
      reinstall: '会先卸载（清空缓存）再重新安装，期间节点不可用。',
      uninstall: '会停止服务并删除程序、配置和缓存，节点记录保留在控制台。',
      stop: '停止后节点不再接收流量（开机仍会自动启动）。',
    }[action]
    if (!(await ask({ title: `${label}节点`, message: `确认${label}：${preview}？${warn}`, confirmText: label, danger: true }))) return
  }
  if (action === 'upgrade' || action === 'install' || action === 'reinstall') {
    pendingAction.value = action
    pendingIds.value = ids
    versionInput.value = ''
    versionOpen.value = true
    return
  }
  await submitOps(action, ids)
}

async function confirmVersion() {
  versionOpen.value = false
  await submitOps(pendingAction.value, pendingIds.value, versionInput.value.trim() || undefined)
}

async function submitOps(action: string, ids: string[], version?: string) {
  busy.value = true
  try {
    const results = await api.post<OpResult[]>('/api/v1/remote/ops', { node_ids: ids, action, version })
    const started = results.filter(r => r.op_id)
    const failed = results.filter(r => r.error)
    if (started.length) toast.success(`已开始${actionLabel(action)} ${started.length} 个节点`)
    for (const f of failed) {
      const name = nodes.value.find(n => n.id === f.node_id)?.name || f.node_id
      toast.error(`${name}：${f.error}`)
    }
    if (started.length === 1 && ids.length === 1) openLog(started[0].op_id!)
    await loadAll()
  } catch (e: any) {
    toast.error(errText(e, `${actionLabel(action)}失败`))
  } finally {
    busy.value = false
  }
}

function onRowAction(n: RemoteNode, event: Event) {
  const el = event.target as HTMLSelectElement
  const action = el.value
  el.value = ''
  if (action) runAction(action, [n.id])
}

// ---- batch add ----
const batchOpen = ref(false)
const batchSubmitting = ref(false)
const batchError = ref('')
const batchResults = ref<BatchResult[]>([])
const batch = reactive({ grant_id: '', region: '', port: 22, lines: '', install: true })

function openBatch() {
  batchResults.value = []
  batchError.value = ''
  batch.grant_id = batch.grant_id || grants.value[0]?.id || ''
  batchOpen.value = true
}

function closeBatch() {
  batchOpen.value = false
  batch.lines = ''
  loadAll()
}

function parseLines() {
  const items: { name: string; host: string; port?: number; public_ip?: string }[] = []
  for (const [i, raw] of batch.lines.split('\n').entries()) {
    const line = raw.trim()
    if (!line || line.startsWith('#')) continue
    const parts = line.split(/[\s,]+/)
    if (parts.length < 2) throw new Error(`第 ${i + 1} 行需要「名称 SSH地址」`)
    let host = parts[1]
    let port: number | undefined
    const m = host.match(/^(.+):(\d+)$/)
    if (m && !host.includes('::')) {
      host = m[1]
      port = Number(m[2])
    }
    items.push({ name: parts[0], host, port, public_ip: parts[2] })
  }
  if (!items.length) throw new Error('请至少填写一个节点')
  return items
}

async function submitBatch() {
  batchError.value = ''
  if (!batch.grant_id) {
    batchError.value = '请选择 SSH 凭证'
    return
  }
  let items
  try {
    items = parseLines()
  } catch (e: any) {
    batchError.value = e.message
    return
  }
  batchSubmitting.value = true
  try {
    batchResults.value = await api.post<BatchResult[]>('/api/v1/remote/nodes/batch', {
      grant_id: batch.grant_id,
      region: batch.region,
      port: Number(batch.port) || 22,
      install: batch.install,
      items,
    })
    loadAll()
  } catch (e: any) {
    batchError.value = errText(e, '添加失败')
  } finally {
    batchSubmitting.value = false
  }
}

// ---- SSH settings ----
const sshOpen = ref(false)
const sshSaving = ref(false)
const sshError = ref('')
const sshNode = ref<RemoteNode | null>(null)
const sshForm = reactive({ host: '', port: 22, grant_id: '' })

function openSsh(n: RemoteNode) {
  sshNode.value = n
  sshForm.host = n.ssh_host || n.public_ip
  sshForm.port = n.ssh_port || 22
  sshForm.grant_id = n.grant_id || grants.value[0]?.id || ''
  sshError.value = ''
  sshOpen.value = true
}

async function saveSsh() {
  if (!sshNode.value) return
  sshSaving.value = true
  sshError.value = ''
  try {
    await api.put(`/api/v1/remote/nodes/${sshNode.value.id}/ssh`, {
      host: sshForm.host.trim(),
      port: Number(sshForm.port) || 22,
      grant_id: sshForm.grant_id,
    })
    toast.success('SSH 设置已保存')
    sshOpen.value = false
    await loadAll()
  } catch (e: any) {
    sshError.value = errText(e, '保存失败')
  } finally {
    sshSaving.value = false
  }
}

async function resetKey() {
  if (!sshNode.value) return
  if (!(await ask({ title: '重置主机指纹', message: '只有确认服务器重装过系统时才需要重置。继续？', confirmText: '重置' }))) return
  try {
    await api.post(`/api/v1/remote/nodes/${sshNode.value.id}/ssh/reset-key`)
    sshNode.value.host_key = ''
    toast.success('已重置，下次连接时重新记录')
    loadAll()
  } catch (e: any) {
    toast.error(errText(e, '重置失败'))
  }
}

// ---- operation log ----
const logOpen = ref(false)
const logOp = ref<Op | null>(null)
const logBox = ref<HTMLElement | null>(null)
let logTimer: ReturnType<typeof setTimeout> | null = null
const logTitle = computed(() => (logOp.value ? `${actionLabel(logOp.value.action)}：${logOp.value.node_name}` : '任务日志'))

async function fetchLog(id: string) {
  try {
    const op = await api.get<Op>(`/api/v1/remote/ops/${id}`)
    const stick = logBox.value ? logBox.value.scrollTop + logBox.value.clientHeight >= logBox.value.scrollHeight - 20 : true
    logOp.value = op
    if (stick) nextTick(() => { if (logBox.value) logBox.value.scrollTop = logBox.value.scrollHeight })
    if (logOpen.value && (op.status === 'queued' || op.status === 'running')) {
      logTimer = setTimeout(() => fetchLog(id), 2000)
    } else if (op.status === 'success' || op.status === 'failed') {
      loadAll()
    }
  } catch (e: any) {
    toast.error(errText(e, '读取日志失败'))
  }
}

function openLog(id: string) {
  if (logTimer) clearTimeout(logTimer)
  logOp.value = null
  logOpen.value = true
  fetchLog(id)
}

watch(logOpen, (open) => {
  if (!open && logTimer) clearTimeout(logTimer)
})

// Keep the list fresh while anything is running.
let listTimer: ReturnType<typeof setInterval> | null = null
const route = useRoute()

async function focusNodeFromQuery() {
  const raw = route.query.node
  const id = typeof raw === 'string' ? raw : Array.isArray(raw) ? raw[0] : ''
  if (!id) return
  const target = nodes.value.find((n) => n.id === id || n.name === id)
  if (!target) return
  selected.value = target.ssh_host ? [target.id] : []
  openSsh(target)
}

onMounted(async () => {
  await loadAll()
  await focusNodeFromQuery()
  listTimer = setInterval(() => {
    const active = nodes.value.some(n => n.last_op && (n.last_op.status === 'queued' || n.last_op.status === 'running'))
    if (active && !loading.value) loadAll()
  }, 4000)
})
onBeforeUnmount(() => {
  if (listTimer) clearInterval(listTimer)
  if (logTimer) clearTimeout(logTimer)
})
</script>

<style scoped>
.op-log {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: calc(100vh - 260px);
  overflow: auto;
  margin: 0;
}
.card :deep(.table td),
.card :deep(.table th) {
  white-space: nowrap;
}
.check-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  cursor: pointer;
}
</style>
