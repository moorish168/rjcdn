<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">默认缓存条件</div>
        <div class="page-desc">
          可复用的路径/TTL 条件包，挂到站点后与本站缓存规则一并生效（本站规则优先）。节点磁盘容量请用「节点 → 缓存策略」，二者不是一回事。
        </div>
      </div>
      <button class="btn btn-primary" type="button" @click="openCreate">新建条件包</button>
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
                  <th>规则数</th>
                  <th>状态</th>
                  <th style="width: 240px">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="p in policies" :key="p.id">
                  <td>
                    <strong>{{ p.name }}</strong>
                    <span v-if="p.is_builtin" class="builtin-tag">内置</span>
                    <div class="text-muted" style="font-size: 12px">{{ p.note || '—' }}</div>
                  </td>
                  <td>{{ p.rule_count }}</td>
                  <td>{{ p.enabled ? '启用' : '停用' }}</td>
                  <td>
                    <button class="btn btn-text btn-sm" type="button" @click="openRules(p)">条件</button>
                    <button class="btn btn-text btn-sm" type="button" @click="openAssign(p)">设为站点默认</button>
                    <button
                      v-if="!p.is_builtin"
                      class="btn btn-text btn-sm danger"
                      type="button"
                      :disabled="busy"
                      @click="removePolicy(p)"
                    >
                      删除
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <EmptyState
            v-else
            icon="rules"
            title="还没有默认缓存条件"
            text="创建条件包后，可在站点缓存页选用；本站缓存规则优先级更高。"
          />
        </template>
      </div>
    </div>

    <BaseModal
      v-model="createOpen"
      title="新建默认缓存条件"
      confirm-text="创建"
      :loading="saving"
      :confirm-disabled="!form.name.trim()"
      @confirm="createPolicy"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" placeholder="例如：静态站长缓存" />
        </div>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="form.note" class="input" />
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-model="rulesOpen"
      :title="`条件：${active?.name || ''}`"
      confirm-text="添加条件"
      :loading="saving"
      :confirm-disabled="!ruleForm.path_pattern.trim()"
      width="720px"
      @confirm="addRule"
    >
      <div class="form-col">
        <div class="field-row">
          <div class="field" style="flex: 2">
            <label class="field-label">路径 <span class="req">*</span></label>
            <input v-model="ruleForm.path_pattern" class="input mono" placeholder="/static/*" />
          </div>
          <div class="field">
            <label class="field-label">动作</label>
            <select v-model="ruleForm.action" class="select">
              <option value="cache">缓存</option>
              <option value="bypass">绕过</option>
              <option value="no_store">不存储</option>
            </select>
          </div>
          <div class="field">
            <label class="field-label">TTL</label>
            <input v-model.number="ruleForm.ttl_seconds" class="input" type="number" />
          </div>
        </div>
        <div v-if="rules.length" class="table-wrap">
          <table class="table">
            <thead>
              <tr>
                <th>路径</th>
                <th>动作</th>
                <th>TTL</th>
                <th>优先级</th>
                <th style="width: 80px">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in rules" :key="r.id">
                <td class="mono">{{ r.path_pattern }}</td>
                <td>{{ r.action }}</td>
                <td>{{ r.ttl_seconds }}</td>
                <td>{{ r.priority }}</td>
                <td>
                  <button class="btn btn-text btn-sm danger" type="button" :disabled="busy" @click="removeRule(r)">
                    删除
                  </button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-model="assignOpen"
      title="设为站点默认缓存条件"
      confirm-text="保存"
      :loading="saving"
      :confirm-disabled="!assign.domain_id"
      @confirm="saveAssign"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">站点 <span class="req">*</span></label>
          <select v-model="assign.domain_id" class="select">
            <option value="" disabled>选择站点</option>
            <option v-for="d in domains" :key="d.id" :value="d.id">{{ siteOptionLabel(d) }}</option>
          </select>
          <span class="field-hint">仅影响站点路径/TTL 条件；节点磁盘请到「节点 → 缓存策略」。</span>
        </div>
      </div>
    </BaseModal>
  </div>
</template>

<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'
import { siteOptionLabel } from '~/utils/siteLabel'

const api = useApi()
const toast = useToast()
const { ask } = useConfirm()

const policies = ref<any[]>([])
const rules = ref<any[]>([])
const domains = ref<any[]>([])
const active = ref<any>(null)
const createOpen = ref(false)
const rulesOpen = ref(false)
const assignOpen = ref(false)
const saving = ref(false)
const loading = ref(false)
const busy = ref(false)
const form = reactive({ name: '', note: '' })
const ruleForm = reactive({ path_pattern: '/static/*', action: 'cache', ttl_seconds: 3600, priority: 10 })
const assign = reactive({ domain_id: '' })

async function load() {
  loading.value = true
  try {
    policies.value = (await api.get<any[]>('/api/v1/cache-policies')) || []
  } catch (e: any) {
    policies.value = []
    toast.error(friendlyErrorMessage(e?.data?.error, '加载默认缓存条件失败'))
  } finally {
    loading.value = false
  }
}

function openCreate() {
  form.name = ''
  form.note = ''
  createOpen.value = true
}

async function createPolicy() {
  if (!form.name.trim()) {
    toast.error('请填写名称')
    return
  }
  saving.value = true
  try {
    await api.post('/api/v1/cache-policies', { ...form, name: form.name.trim() })
    toast.success('已创建')
    createOpen.value = false
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '创建失败'))
  } finally {
    saving.value = false
  }
}

async function openRules(p: any) {
  active.value = p
  try {
    rules.value = (await api.get<any[]>(`/api/v1/cache-policies/${p.id}/rules`)) || []
  } catch (e: any) {
    rules.value = []
    toast.error(friendlyErrorMessage(e?.data?.error, '加载条件失败'))
  }
  rulesOpen.value = true
}

async function addRule() {
  if (!active.value) return
  if (!ruleForm.path_pattern.trim()) {
    toast.error('请填写路径，例如 /static/*')
    return
  }
  saving.value = true
  try {
    await api.post(`/api/v1/cache-policies/${active.value.id}/rules`, {
      ...ruleForm,
      path_pattern: ruleForm.path_pattern.trim(),
    })
    toast.success('条件已添加')
    rules.value =
      (await api.get<any[]>(`/api/v1/cache-policies/${active.value.id}/rules`).catch(() => [])) || []
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '添加失败'))
  } finally {
    saving.value = false
  }
}

async function removeRule(r: any) {
  if (!active.value || busy.value) return
  const ok = await ask({
    title: '删除条件',
    message: `确认删除路径「${r.path_pattern}」？`,
    danger: true,
    confirmText: '删除',
  })
  if (!ok) return
  busy.value = true
  try {
    await api.del(`/api/v1/cache-policies/${active.value.id}/rules/${r.id}`)
    rules.value = rules.value.filter((x) => x.id !== r.id)
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
  assign.domain_id = ''
  domains.value = (await api.get<any[]>('/api/v1/domains').catch(() => [])) || []
  assignOpen.value = true
}

async function saveAssign() {
  if (!active.value || !assign.domain_id) return
  saving.value = true
  try {
    await api.put(`/api/v1/domains/${assign.domain_id}/cache-policy`, {
      cache_policy_id: active.value.id,
    })
    toast.success('已设为该站点默认缓存条件')
    assignOpen.value = false
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '保存失败'))
  } finally {
    saving.value = false
  }
}

async function removePolicy(p: any) {
  if (busy.value || p.is_builtin) return
  if (
    !(await ask({
      title: '删除条件包',
      message: `确认删除「${p.name}」？已挂接站点将不再使用该默认条件。`,
      danger: true,
      confirmText: '删除',
    }))
  )
    return
  busy.value = true
  try {
    await api.del(`/api/v1/cache-policies/${p.id}`)
    toast.success('已删除')
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '删除失败'))
  } finally {
    busy.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.builtin-tag {
  display: inline-block;
  margin-left: 8px;
  padding: 1px 6px;
  font-size: 11px;
  border-radius: 4px;
  background: var(--color-bg-muted, #f1f5f9);
  color: var(--color-text-muted, #64748b);
  vertical-align: middle;
}
.field-row {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}
.field-row .field {
  flex: 1;
  min-width: 120px;
}
</style>
