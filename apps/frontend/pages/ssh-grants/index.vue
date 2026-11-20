<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">SSH 凭证</div>
        <div class="page-desc">
          安全凭证：边缘 / L2 / DNS 远程安装共用；密码和私钥加密保存，不会回显
        </div>
      </div>
      <div v-if="auth.isAdmin" class="page-actions">
        <button class="btn btn-default" type="button" :disabled="loading" @click="load">
          <AppIcon name="refresh" :size="15" />
          刷新
        </button>
        <button class="btn btn-primary" type="button" @click="openCreate">
          <AppIcon name="plus" :size="15" />
          添加凭证
        </button>
      </div>
    </div>

    <div v-if="!auth.isAdmin" class="alert alert-warning">
      <AppIcon name="shield" :size="16" class="icon" />
      <span>只有管理员可以管理 SSH 凭证。</span>
    </div>

    <div v-else class="card">
      <div class="card-body">
        <div v-if="loading" class="skeleton" style="height: 180px" />
        <div v-else-if="grants.length" class="table-wrap">
          <table class="table">
            <thead>
              <tr>
                <th>名称</th>
                <th>登录方式</th>
                <th>用户名</th>
                <th>sudo</th>
                <th>引用数</th>
                <th>备注</th>
                <th style="width: 200px">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="g in grants" :key="g.id">
                <td>{{ g.name }}</td>
                <td>
                  <StatusTag :tone="g.method === 'key' ? 'info' : 'neutral'">
                    {{ g.method === 'key' ? '私钥' : '密码' }}
                  </StatusTag>
                </td>
                <td class="mono">{{ g.username }}</td>
                <td>{{ g.username === 'root' ? '—' : g.use_sudo ? '是' : '否' }}</td>
                <td>{{ g.node_count }}</td>
                <td class="text-muted">{{ g.description || '—' }}</td>
                <td>
                  <div class="flex gap-8">
                    <button class="btn btn-text btn-sm" type="button" @click="openTest(g)">测试</button>
                    <button class="btn btn-text btn-sm" type="button" @click="openEdit(g)">编辑</button>
                    <button class="btn btn-text btn-sm danger" type="button" @click="remove(g)">删除</button>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <EmptyState
          v-else
          icon="shield"
          title="还没有 SSH 凭证"
          text="添加一组全局账号后，可在边缘远程安装或智能 DNS 节点远程安装中选用。"
        />
      </div>
    </div>

    <BaseModal
      v-model="editOpen"
      :title="editing ? '编辑 SSH 凭证' : '添加 SSH 凭证'"
      confirm-text="保存"
      width="560px"
      :loading="submitting"
      @confirm="save"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" placeholder="例如：香港节点 root" />
        </div>
        <div class="field">
          <label class="field-label">登录方式</label>
          <select v-model="form.method" class="select">
            <option value="password">用户名 + 密码</option>
            <option value="key">用户名 + 私钥</option>
          </select>
        </div>
        <div class="field">
          <label class="field-label">用户名 <span class="req">*</span></label>
          <input v-model="form.username" class="input mono" placeholder="root" autocomplete="off" />
        </div>
        <div v-if="form.method === 'password'" class="field">
          <label class="field-label">密码 <span v-if="!editing" class="req">*</span></label>
          <input
            v-model="form.password"
            class="input"
            type="password"
            autocomplete="new-password"
            :placeholder="editing ? '留空表示不修改' : ''"
          />
        </div>
        <template v-else>
          <div class="field">
            <label class="field-label">私钥 <span v-if="!editing" class="req">*</span></label>
            <textarea
              v-model="form.private_key"
              class="textarea mono"
              rows="6"
              spellcheck="false"
              :placeholder="editing ? '留空表示不修改' : '-----BEGIN OPENSSH PRIVATE KEY-----'"
            />
          </div>
          <div class="field">
            <label class="field-label">私钥密码</label>
            <input
              v-model="form.passphrase"
              class="input"
              type="password"
              autocomplete="new-password"
              placeholder="私钥没有密码可留空"
            />
          </div>
        </template>
        <label v-if="form.username.trim() !== 'root'" class="check-row">
          <input v-model="form.use_sudo" type="checkbox" />
          <span>非 root 用户，通过 sudo 执行安装（私钥登录需要免密 sudo）</span>
        </label>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="form.description" class="input" />
        </div>
        <p v-if="error" class="text-danger" style="font-size: 13px">{{ error }}</p>
      </div>
    </BaseModal>

    <BaseModal
      v-model="testOpen"
      :title="`测试连接：${testing?.name || ''}`"
      confirm-text="开始测试"
      :loading="testRunning"
      @confirm="runTest"
    >
      <div class="form-col">
        <div class="flex gap-8">
          <div class="field" style="flex: 1">
            <label class="field-label">服务器地址</label>
            <input v-model="testForm.host" class="input mono" placeholder="203.0.113.21" />
          </div>
          <div class="field" style="width: 110px">
            <label class="field-label">SSH 端口</label>
            <input v-model.number="testForm.port" class="input mono" type="number" min="1" max="65535" />
          </div>
        </div>
        <div v-if="testResult" class="alert" :class="testResult.ok ? 'alert-success' : 'alert-danger'">
          <div>
            <div><strong>{{ testResult.message }}</strong></div>
            <div v-if="testResult.privilege" style="font-size: 13px">权限：{{ testResult.privilege }}</div>
            <div v-if="testResult.system" style="font-size: 13px">系统：{{ testResult.system }}</div>
            <div v-if="testResult.fingerprint" class="mono" style="font-size: 12px">
              指纹：{{ testResult.fingerprint }}
            </div>
          </div>
        </div>
      </div>
    </BaseModal>
  </div>
</template>

<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'

const api = useApi()
const auth = useAuthStore()
const toast = useToast()
const { ask } = useConfirm()

interface Grant {
  id: string
  name: string
  method: 'password' | 'key'
  username: string
  use_sudo: boolean
  description: string
  has_passphrase: boolean
  node_count: number
}

interface TestResult {
  ok: boolean
  message: string
  fingerprint: string
  privilege: string
  system: string
}

const grants = ref<Grant[]>([])
const loading = ref(false)
const editOpen = ref(false)
const submitting = ref(false)
const editing = ref<Grant | null>(null)
const error = ref('')
const form = reactive({
  name: '',
  method: 'password' as 'password' | 'key',
  username: 'root',
  password: '',
  private_key: '',
  passphrase: '',
  use_sudo: false,
  description: '',
})

const testOpen = ref(false)
const testRunning = ref(false)
const testing = ref<Grant | null>(null)
const testForm = reactive({ host: '', port: 22 })
const testResult = ref<TestResult | null>(null)

function errText(e: any, fallback: string) {
  return friendlyErrorMessage(e?.data?.error || e?.message || e, fallback)
}

async function load() {
  if (!auth.isAdmin) return
  loading.value = true
  try {
    grants.value = await api.get<Grant[]>('/api/v1/remote/grants')
  } catch (e: any) {
    toast.error(errText(e, '读取 SSH 凭证失败'))
  } finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  Object.assign(form, {
    name: '',
    method: 'password',
    username: 'root',
    password: '',
    private_key: '',
    passphrase: '',
    use_sudo: false,
    description: '',
  })
  error.value = ''
  editOpen.value = true
}

function openEdit(g: Grant) {
  editing.value = g
  Object.assign(form, {
    name: g.name,
    method: g.method,
    username: g.username,
    password: '',
    private_key: '',
    passphrase: '',
    use_sudo: g.use_sudo,
    description: g.description,
  })
  error.value = ''
  editOpen.value = true
}

async function save() {
  error.value = ''
  submitting.value = true
  const body = {
    name: form.name,
    method: form.method,
    username: form.username,
    password: form.method === 'password' ? form.password : undefined,
    private_key: form.method === 'key' ? form.private_key : undefined,
    passphrase: form.method === 'key' ? form.passphrase : undefined,
    use_sudo: form.username.trim() !== 'root' && form.use_sudo,
    description: form.description,
  }
  try {
    if (editing.value) {
      await api.put(`/api/v1/remote/grants/${editing.value.id}`, body)
      toast.success('SSH 凭证已更新')
    } else {
      await api.post('/api/v1/remote/grants', body)
      toast.success('SSH 凭证已创建')
    }
    editOpen.value = false
    await load()
  } catch (e: any) {
    error.value = errText(e, '保存失败')
  } finally {
    submitting.value = false
  }
}

async function remove(g: Grant) {
  if (!(await ask({
    title: '删除 SSH 凭证',
    message: `确认删除「${g.name}」？正在被节点使用的认证不能删除。`,
    confirmText: '删除',
    danger: true,
  }))) return
  try {
    await api.del(`/api/v1/remote/grants/${g.id}`)
    toast.success('已删除')
    await load()
  } catch (e: any) {
    toast.error(errText(e, '删除失败'))
  }
}

function openTest(g: Grant) {
  testing.value = g
  testResult.value = null
  testOpen.value = true
}

async function runTest() {
  if (!testing.value || !testForm.host.trim()) {
    toast.error('请填写服务器地址')
    return
  }
  testRunning.value = true
  testResult.value = null
  try {
    testResult.value = await api.post<TestResult>(`/api/v1/remote/grants/${testing.value.id}/test`, {
      host: testForm.host.trim(),
      port: Number(testForm.port) || 22,
    })
  } catch (e: any) {
    testResult.value = { ok: false, message: errText(e, '测试失败'), fingerprint: '', privilege: '', system: '' }
  } finally {
    testRunning.value = false
  }
}

onMounted(load)
</script>

<style scoped>
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
