<script setup lang="ts">
const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const dns = inject('dns') as ReturnType<typeof useDnsConsole> | undefined

interface Provider {
  id: string
  name: string
  provider_type: string
  note: string
  enabled: boolean
  credentials_configured: boolean
}

const TYPE_LABEL: Record<string, string> = {
  cloudflare: 'Cloudflare',
  alidns: '阿里云 DNS',
  dnspod: 'DNSPod / 腾讯云',
}

const loading = ref(false)
const providers = ref<Provider[]>([])
const open = ref(false)
const saving = ref(false)
const editing = ref<Provider | null>(null)
const form = reactive({
  name: '',
  provider_type: 'cloudflare',
  note: '',
  enabled: true,
  api_token: '',
  access_key_id: '',
  access_key_secret: '',
  secret_id: '',
  secret_key: '',
  api_type: 'tencent',
})

async function load() {
  loading.value = true
  try {
    providers.value = (await api.get<Provider[]>('/api/v1/dns/providers')) || []
  } catch (e: any) {
    toast.error(e?.data?.error || '加载失败')
  } finally {
    loading.value = false
  }
}

function openCreate() {
  editing.value = null
  form.name = ''
  form.provider_type = 'cloudflare'
  form.note = ''
  form.enabled = true
  form.api_token = ''
  form.access_key_id = ''
  form.access_key_secret = ''
  form.secret_id = ''
  form.secret_key = ''
  form.api_type = 'tencent'
  open.value = true
}

function openEdit(p: Provider) {
  editing.value = p
  form.name = p.name
  form.provider_type = p.provider_type
  form.note = p.note || ''
  form.enabled = p.enabled
  form.api_token = ''
  form.access_key_id = ''
  form.access_key_secret = ''
  form.secret_id = ''
  form.secret_key = ''
  form.api_type = 'tencent'
  open.value = true
}

function credentialsPayload() {
  if (form.provider_type === 'cloudflare') {
    if (!form.api_token.trim() && editing.value) return undefined
    return { api_token: form.api_token.trim() }
  }
  if (form.provider_type === 'alidns') {
    if (!form.access_key_id.trim() && editing.value) return undefined
    return {
      access_key_id: form.access_key_id.trim(),
      access_key_secret: form.access_key_secret.trim(),
    }
  }
  if (!form.secret_id.trim() && editing.value) return undefined
  return {
    secret_id: form.secret_id.trim(),
    secret_key: form.secret_key.trim(),
    api_type: form.api_type,
  }
}

async function save() {
  if (!form.name.trim()) {
    toast.error('请填写账号名称')
    return
  }
  const credentials = credentialsPayload()
  if (!editing.value && !credentials) {
    toast.error('请填写 API 凭证')
    return
  }
  saving.value = true
  try {
    if (editing.value) {
      await api.put(`/api/v1/dns/providers/${editing.value.id}`, {
        name: form.name.trim(),
        note: form.note.trim(),
        enabled: form.enabled,
        credentials,
      })
      toast.success('已保存')
    } else {
      await api.post('/api/v1/dns/providers', {
        name: form.name.trim(),
        provider_type: form.provider_type,
        note: form.note.trim(),
        credentials,
      })
      toast.success('已添加')
    }
    open.value = false
    await load()
  } catch (e: any) {
    toast.error(e?.data?.error || '保存失败')
  } finally {
    saving.value = false
  }
}

async function remove(p: Provider) {
  const ok = await ask({
    title: '删除 DNS 服务商',
    message: `确定删除「${p.name}」？已绑定该账号的站点将无法自动签发泛域名证书。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  try {
    await api.del(`/api/v1/dns/providers/${p.id}`)
    toast.success('已删除')
    await load()
  } catch (e: any) {
    toast.error(e?.data?.error || '删除失败')
  }
}

onMounted(() => {
  load()
  dns?.registerProvidersReload?.(load)
})
onBeforeUnmount(() => {
  dns?.registerProvidersReload?.(null)
})
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">DNS 服务商</div>
        <div class="card-desc">
          CNAME 接入站点若需要泛域名证书，须绑定客户 DNS 账号，由平台代写 ACME TXT（对齐 GoEdge）
        </div>
      </div>
      <div class="card-extra flex gap-8">
        <button class="btn btn-default btn-sm" type="button" :disabled="loading" @click="load">
          <AppIcon name="refresh" :size="14" />
          刷新
        </button>
        <button class="btn btn-primary btn-sm" type="button" @click="openCreate">添加账号</button>
      </div>
    </div>
    <div v-if="loading" class="card-body text-muted">加载中…</div>
    <div v-else-if="providers.length" class="table-wrap">
      <table class="table">
        <thead>
          <tr>
            <th>名称</th>
            <th>厂商</th>
            <th>凭证</th>
            <th>状态</th>
            <th>备注</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in providers" :key="p.id">
            <td>{{ p.name }}</td>
            <td>{{ TYPE_LABEL[p.provider_type] || p.provider_type }}</td>
            <td>
              <StatusTag :tone="p.credentials_configured ? 'success' : 'warning'" dot>
                {{ p.credentials_configured ? '已配置' : '缺失' }}
              </StatusTag>
            </td>
            <td>
              <StatusTag :status="p.enabled ? 'active' : 'disabled'">
                {{ p.enabled ? '启用' : '停用' }}
              </StatusTag>
            </td>
            <td class="text-2">{{ p.note || '—' }}</td>
            <td class="text-right">
              <button class="btn btn-text btn-sm" type="button" @click="openEdit(p)">编辑</button>
              <button class="btn btn-text btn-sm text-danger" type="button" @click="remove(p)">删除</button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <EmptyState
      v-else
      icon="globe"
      title="暂无 DNS 服务商账号"
      text="整域托管不需要此项；CNAME + 泛域名证书请先添加 Cloudflare / 阿里云 / DNSPod 账号。"
    >
      <button class="btn btn-primary btn-sm" type="button" @click="openCreate">添加账号</button>
    </EmptyState>
  </div>

  <BaseModal
    v-model="open"
    :title="editing ? '编辑 DNS 服务商' : '添加 DNS 服务商'"
    confirm-text="保存"
    :loading="saving"
    @confirm="save"
  >
    <div class="form-col">
      <div class="field">
        <label class="field-label">账号名称</label>
        <input v-model="form.name" class="input" maxlength="64" placeholder="例如 客户A-阿里云" />
      </div>
      <div v-if="!editing" class="field">
        <label class="field-label">厂商</label>
        <select v-model="form.provider_type" class="select">
          <option value="cloudflare">Cloudflare</option>
          <option value="alidns">阿里云 DNS</option>
          <option value="dnspod">DNSPod / 腾讯云</option>
        </select>
      </div>
      <template v-if="form.provider_type === 'cloudflare'">
        <div class="field">
          <label class="field-label">API Token</label>
          <input
            v-model="form.api_token"
            class="input mono"
            type="password"
            :placeholder="editing ? '留空表示不修改' : 'Zone:DNS:Edit'"
            autocomplete="new-password"
          />
        </div>
      </template>
      <template v-else-if="form.provider_type === 'alidns'">
        <div class="field">
          <label class="field-label">AccessKeyId</label>
          <input
            v-model="form.access_key_id"
            class="input mono"
            :placeholder="editing ? '留空表示不修改' : ''"
            autocomplete="off"
          />
        </div>
        <div class="field">
          <label class="field-label">AccessKeySecret</label>
          <input
            v-model="form.access_key_secret"
            class="input mono"
            type="password"
            :placeholder="editing ? '留空表示不修改' : ''"
            autocomplete="new-password"
          />
        </div>
      </template>
      <template v-else>
        <div class="field">
          <label class="field-label">密钥类型</label>
          <select v-model="form.api_type" class="select">
            <option value="tencent">腾讯云 API 密钥</option>
            <option value="dnspod_token">DNSPod Token</option>
          </select>
        </div>
        <div class="field">
          <label class="field-label">{{ form.api_type === 'dnspod_token' ? '密钥 ID' : 'SecretId' }}</label>
          <input
            v-model="form.secret_id"
            class="input mono"
            :placeholder="editing ? '留空表示不修改' : ''"
            autocomplete="off"
          />
        </div>
        <div class="field">
          <label class="field-label">{{ form.api_type === 'dnspod_token' ? 'Token' : 'SecretKey' }}</label>
          <input
            v-model="form.secret_key"
            class="input mono"
            type="password"
            :placeholder="editing ? '留空表示不修改' : ''"
            autocomplete="new-password"
          />
        </div>
      </template>
      <div class="field">
        <label class="field-label">备注</label>
        <input v-model="form.note" class="input" maxlength="200" />
      </div>
      <label v-if="editing" class="check">
        <input v-model="form.enabled" type="checkbox" />
        启用
      </label>
    </div>
  </BaseModal>
</template>
