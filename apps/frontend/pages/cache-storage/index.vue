<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'

const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const auth = useAuthStore()

const policies = ref<any[]>([])
const nodes = ref<any[]>([])
const loading = ref(false)
const saving = ref(false)
const busy = ref(false)

const createOpen = ref(false)
const editOpen = ref(false)
const assignOpen = ref(false)
const active = ref<any | null>(null)

const form = reactive({
  name: '',
  note: '',
  enabled: true,
  large_cache_enabled: true,
  disk_dir: '/var/lib/lagran/large-cache',
  disk_max_gb: 0,
  memory_max_mb: 0,
})

const assignNodeId = ref('')

function bytesToGb(n: number) {
  if (!n) return 0
  return Math.round((n / (1024 ** 3)) * 100) / 100
}
function gbToBytes(n: number) {
  if (!n || n <= 0) return 0
  return Math.round(n * 1024 ** 3)
}
function bytesToMb(n: number) {
  if (!n) return 0
  return Math.round((n / (1024 ** 2)) * 10) / 10
}
function mbToBytes(n: number) {
  if (!n || n <= 0) return 0
  return Math.round(n * 1024 ** 2)
}

function formatCap(bytes: number, kind: 'disk' | 'mem') {
  if (!bytes) return kind === 'disk' ? '沿用硬件画像' : '沿用默认'
  if (kind === 'disk') return `${bytesToGb(bytes)} GiB`
  return `${bytesToMb(bytes)} MiB`
}

async function load() {
  loading.value = true
  try {
    policies.value = (await api.get<any[]>('/api/v1/cache-storage-policies')) || []
  } catch (e: any) {
    policies.value = []
    toast.error(friendlyErrorMessage(e?.data?.error || e?.message, '加载缓存策略失败'))
  } finally {
    loading.value = false
  }
}

function openCreate() {
  Object.assign(form, {
    name: '',
    note: '',
    enabled: true,
    large_cache_enabled: true,
    disk_dir: '/var/lib/lagran/large-cache',
    disk_max_gb: 0,
    memory_max_mb: 0,
  })
  createOpen.value = true
}

function openEdit(p: any) {
  active.value = p
  Object.assign(form, {
    name: p.name,
    note: p.note || '',
    enabled: p.enabled,
    large_cache_enabled: p.large_cache_enabled,
    disk_dir: p.disk_dir || '/var/lib/lagran/large-cache',
    disk_max_gb: bytesToGb(p.disk_max_bytes || 0),
    memory_max_mb: bytesToMb(p.memory_max_bytes || 0),
  })
  editOpen.value = true
}

function payload() {
  return {
    name: form.name.trim(),
    note: form.note.trim(),
    enabled: form.enabled,
    large_cache_enabled: form.large_cache_enabled,
    disk_dir: form.disk_dir.trim(),
    disk_max_bytes: gbToBytes(Number(form.disk_max_gb) || 0),
    memory_max_bytes: mbToBytes(Number(form.memory_max_mb) || 0),
  }
}

async function createPolicy() {
  saving.value = true
  try {
    await api.post('/api/v1/cache-storage-policies', payload())
    toast.success('策略已创建')
    createOpen.value = false
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '创建失败'))
  } finally {
    saving.value = false
  }
}

async function saveEdit() {
  if (!active.value) return
  saving.value = true
  try {
    await api.put(`/api/v1/cache-storage-policies/${active.value.id}`, payload())
    toast.success('策略已更新；已绑定节点的画像已同步')
    editOpen.value = false
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '保存失败'))
  } finally {
    saving.value = false
  }
}

async function removePolicy(p: any) {
  if (busy.value) return
  if (!(await ask({ title: '删除策略', message: `确认删除「${p.name}」？`, danger: true, confirmText: '删除' })))
    return
  busy.value = true
  try {
    await api.del(`/api/v1/cache-storage-policies/${p.id}`)
    toast.success('已删除')
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '删除失败'))
  } finally {
    busy.value = false
  }
}

async function openAssign(p: any) {
  active.value = p
  assignNodeId.value = ''
  nodes.value = (await api.get<any[]>('/api/v1/nodes').catch(() => [])) || []
  assignOpen.value = true
}

async function saveAssign() {
  if (!active.value || !assignNodeId.value) return
  saving.value = true
  try {
    await api.put(`/api/v1/nodes/${assignNodeId.value}/cache-storage-policy`, {
      cache_storage_policy_id: active.value.id,
    })
    toast.success('已绑定到节点，并写入 LARGE_CACHE_* 到硬件画像')
    assignOpen.value = false
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '绑定失败'))
  } finally {
    saving.value = false
  }
}

onMounted(() => {
  if (!auth.isAdmin) {
    navigateTo('/nodes', { replace: true })
    return
  }
  load()
})
</script>

<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">缓存策略</div>
        <div class="page-desc">
          给节点用：磁盘目录、容量上限。站点「缓存什么」请用站点缓存规则 / 默认缓存条件。
        </div>
      </div>
      <button class="btn btn-primary" type="button" @click="openCreate">添加策略</button>
    </div>

    <div class="card">
      <div class="card-body">
        <div v-if="loading && !policies.length" class="text-muted">正在加载…</div>
        <template v-else>
          <div v-if="policies.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>名称</th>
                  <th>磁盘目录</th>
                  <th>磁盘上限</th>
                  <th>内存上限</th>
                  <th>节点</th>
                  <th>状态</th>
                  <th style="width: 220px">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="p in policies" :key="p.id">
                  <td>
                    <strong>{{ p.name }}</strong>
                    <div class="text-muted" style="font-size: 12px">{{ p.note || '—' }}</div>
                  </td>
                  <td class="mono" style="font-size: 12px">{{ p.disk_dir }}</td>
                  <td>{{ formatCap(p.disk_max_bytes, 'disk') }}</td>
                  <td>{{ formatCap(p.memory_max_bytes, 'mem') }}</td>
                  <td>{{ p.node_count }}</td>
                  <td>{{ p.enabled ? (p.large_cache_enabled ? '启用' : '已关大对象缓存') : '停用' }}</td>
                  <td>
                    <button class="btn btn-text btn-sm" type="button" @click="openEdit(p)">编辑</button>
                    <button class="btn btn-text btn-sm" type="button" @click="openAssign(p)">分配节点</button>
                    <button class="btn btn-text btn-sm danger" type="button" :disabled="busy" @click="removePolicy(p)">删除</button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <EmptyState v-else icon="node" title="还没有缓存策略" text="创建后可在节点设置中绑定，写入磁盘目录与容量。" />
        </template>
      </div>
    </div>

    <BaseModal
      v-model="createOpen"
      title="添加缓存策略"
      confirm-text="创建"
      :loading="saving"
      :confirm-disabled="!form.name.trim()"
      @confirm="createPolicy"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" placeholder="默认磁盘缓存" />
        </div>
        <div class="field">
          <label class="field-label">磁盘目录</label>
          <input v-model="form.disk_dir" class="input mono" placeholder="/var/lib/lagran/large-cache" />
        </div>
        <div class="field-row">
          <div class="field">
            <label class="field-label">磁盘上限（GiB）</label>
            <input v-model.number="form.disk_max_gb" class="input" type="number" min="0" step="1" />
            <span class="field-hint">0 = 沿用硬件画像推算的 LARGE_CACHE_MAX_BYTES</span>
          </div>
          <div class="field">
            <label class="field-label">内存上限（MiB）</label>
            <input v-model.number="form.memory_max_mb" class="input" type="number" min="0" step="1" />
            <span class="field-hint">0 = 不覆盖；写入 COMPRESSED_CACHE_BYTES</span>
          </div>
        </div>
        <label class="check-row"><input v-model="form.large_cache_enabled" type="checkbox" /> 启用磁盘大对象缓存</label>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="form.note" class="input" />
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-model="editOpen"
      title="编辑缓存策略"
      confirm-text="保存"
      :loading="saving"
      :confirm-disabled="!form.name.trim()"
      @confirm="saveEdit"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" />
        </div>
        <div class="field">
          <label class="field-label">磁盘目录</label>
          <input v-model="form.disk_dir" class="input mono" />
        </div>
        <div class="field-row">
          <div class="field">
            <label class="field-label">磁盘上限（GiB）</label>
            <input v-model.number="form.disk_max_gb" class="input" type="number" min="0" step="1" />
          </div>
          <div class="field">
            <label class="field-label">内存上限（MiB）</label>
            <input v-model.number="form.memory_max_mb" class="input" type="number" min="0" step="1" />
          </div>
        </div>
        <label class="check-row"><input v-model="form.large_cache_enabled" type="checkbox" /> 启用磁盘大对象缓存</label>
        <label class="check-row"><input v-model="form.enabled" type="checkbox" /> 启用此策略</label>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="form.note" class="input" />
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-model="assignOpen"
      title="分配到节点"
      confirm-text="绑定"
      :loading="saving"
      :confirm-disabled="!assignNodeId"
      @confirm="saveAssign"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">节点</label>
          <select v-model="assignNodeId" class="select">
            <option value="" disabled>选择节点</option>
            <option v-for="n in nodes" :key="n.id" :value="n.id">{{ n.name }}（{{ n.public_ip || n.ip }}）</option>
          </select>
          <span class="field-hint">绑定后会把目录/容量写入该节点硬件画像中的 edge.env。</span>
        </div>
      </div>
    </BaseModal>
  </div>
</template>
