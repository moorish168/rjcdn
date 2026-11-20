<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'

const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const id = inject<Ref<string>>('domainId')!
const reloadDomain = inject<(() => Promise<void>) | undefined>('reloadDomain', undefined)

const loading = ref(false)
const cacheBusy = ref(false)
const saving = ref(false)
const busy = ref(false)

const cacheEnabled = ref(true)
const useDefault = ref(true)
const selectedPolicyId = ref('')
const policies = ref<any[]>([])
const policyRules = ref<any[]>([])
const siteRules = ref<any[]>([])

const formOpen = ref(false)
const editing = ref<any>(null)
const form = ref({
  path_pattern: '/*',
  match_type: 'glob',
  action: 'cache',
  ttl_seconds: 60,
  swr_seconds: 0,
  browser_ttl_seconds: 0,
  priority: 10,
  enabled: true,
  note: '',
  ignore_cookie: false,
})

const selectedPolicy = computed(() => policies.value.find((p) => p.id === selectedPolicyId.value) || null)

async function load() {
  loading.value = true
  try {
    const [settings, pols, allRules] = await Promise.all([
      api.get<any>(`/api/v1/domains/${id.value}/settings`),
      api.get<any[]>('/api/v1/cache-policies').catch(() => []),
      api.get<any[]>('/api/v1/rules').catch(() => []),
    ])
    cacheEnabled.value = settings?.cache_enabled !== false
    policies.value = pols || []
    const bound = settings?.cache_policy_id || ''
    useDefault.value = Boolean(bound)
    selectedPolicyId.value =
      bound || policies.value.find((p) => p.is_builtin)?.id || policies.value[0]?.id || ''
    siteRules.value = (allRules || []).filter((r) => r.domain_id === id.value)
    await loadPolicyRules()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '加载缓存设置失败'))
  } finally {
    loading.value = false
  }
}

async function loadPolicyRules() {
  if (!useDefault.value || !selectedPolicyId.value) {
    policyRules.value = []
    return
  }
  try {
    policyRules.value =
      (await api.get<any[]>(`/api/v1/cache-policies/${selectedPolicyId.value}/rules`)) || []
  } catch {
    policyRules.value = []
  }
}

watch(selectedPolicyId, () => {
  if (useDefault.value) loadPolicyRules()
})

async function toggleCache() {
  if (cacheBusy.value || loading.value) return
  const next = !cacheEnabled.value
  const ok = await ask({
    title: next ? '开启站点缓存' : '关闭站点缓存',
    message: next
      ? '开启后该站点将按缓存规则与默认缓存条件读写缓存。确认继续？'
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
    cacheEnabled.value = s.cache_enabled !== false
    toast.success(next ? '已开启站点缓存' : '已关闭缓存并下发清理')
    await reloadDomain?.()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '切换失败'))
  } finally {
    cacheBusy.value = false
  }
}

async function onUseDefaultChange() {
  if (!useDefault.value) {
    saving.value = true
    try {
      await api.put(`/api/v1/domains/${id.value}/cache-policy`, { cache_policy_id: null })
      policyRules.value = []
      toast.success('已关闭默认缓存条件')
    } catch (e: any) {
      useDefault.value = true
      toast.error(friendlyErrorMessage(e?.data?.error, '保存失败'))
    } finally {
      saving.value = false
    }
    return
  }
  if (!selectedPolicyId.value) {
    selectedPolicyId.value = policies.value.find((p) => p.is_builtin)?.id || policies.value[0]?.id || ''
  }
  await savePolicyBind()
}

async function savePolicyBind() {
  if (!selectedPolicyId.value) {
    toast.error('请选择一套默认缓存条件')
    return
  }
  saving.value = true
  try {
    await api.put(`/api/v1/domains/${id.value}/cache-policy`, {
      cache_policy_id: selectedPolicyId.value,
    })
    await loadPolicyRules()
    toast.success('已使用默认缓存条件')
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '保存失败'))
  } finally {
    saving.value = false
  }
}

function actionLabel(action: string) {
  return action === 'bypass' ? '绕过' : action === 'no_store' ? '不存储' : '缓存'
}

function actionTone(action: string) {
  return action === 'bypass' ? 'warning' : action === 'no_store' ? 'danger' : 'success'
}

function openCreate() {
  editing.value = null
  form.value = {
    path_pattern: '/*',
    match_type: 'glob',
    action: 'cache',
    ttl_seconds: 60,
    swr_seconds: 0,
    browser_ttl_seconds: 0,
    priority: 10,
    enabled: true,
    note: '',
    ignore_cookie: false,
  }
  formOpen.value = true
}

function openEdit(rule: any) {
  editing.value = rule
  form.value = {
    path_pattern: rule.path_pattern,
    match_type: rule.match_type || 'glob',
    action: rule.action,
    ttl_seconds: rule.ttl_seconds,
    swr_seconds: rule.swr_seconds || 0,
    browser_ttl_seconds: rule.browser_ttl_seconds || 0,
    priority: rule.priority,
    enabled: rule.enabled !== false,
    note: rule.note || '',
    ignore_cookie: !!rule.ignore_cookie,
  }
  formOpen.value = true
}

async function submitRule() {
  if (!form.value.path_pattern.trim()) {
    toast.error('请填写路径')
    return
  }
  saving.value = true
  const body = {
    path_pattern: form.value.path_pattern.trim(),
    match_type: form.value.match_type,
    action: form.value.action,
    ttl_seconds: Number(form.value.ttl_seconds) || 0,
    swr_seconds: Number(form.value.swr_seconds) || 0,
    browser_ttl_seconds: Number(form.value.browser_ttl_seconds) || 0,
    priority: Number(form.value.priority) || 0,
    enabled: form.value.enabled,
    note: form.value.note.trim(),
    ignore_cookie: form.value.ignore_cookie,
  }
  try {
    if (editing.value) {
      await api.put(`/api/v1/rules/${editing.value.id}`, body)
      toast.success('规则已更新')
    } else {
      await api.post('/api/v1/rules', { ...body, domain_id: id.value })
      toast.success('规则已创建')
    }
    formOpen.value = false
    const all = (await api.get<any[]>('/api/v1/rules').catch(() => [])) || []
    siteRules.value = all.filter((r) => r.domain_id === id.value)
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '保存失败'))
  } finally {
    saving.value = false
  }
}

async function removeRule(rule: any) {
  if (busy.value) return
  const ok = await ask({
    title: '删除本站规则',
    message: `确认删除「${rule.path_pattern}」？`,
    danger: true,
    confirmText: '删除',
  })
  if (!ok) return
  busy.value = true
  try {
    await api.del(`/api/v1/rules/${rule.id}`)
    siteRules.value = siteRules.value.filter((r) => r.id !== rule.id)
    toast.success('已删除')
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '删除失败'))
  } finally {
    busy.value = false
  }
}

watch(id, load, { immediate: true })
</script>

<template>
  <div class="site-cache">
    <div v-if="loading" class="card">
      <div class="card-body text-muted">加载中…</div>
    </div>

    <template v-else>
      <div class="card">
        <div class="card-head">
          <div>
            <div class="card-title">启用缓存</div>
            <div class="card-desc">关闭后该站点不读写缓存，并清理已有缓存。磁盘容量在「节点 → 缓存策略」。</div>
          </div>
        </div>
        <div class="card-body">
          <label
            class="switch"
            :class="{ on: cacheEnabled, disabled: cacheBusy }"
            @click.prevent="toggleCache"
          >
            <span class="switch-track" />
            {{ cacheEnabled ? '已启用' : '已关闭' }}
          </label>
        </div>
      </div>

      <div class="card" :class="{ dimmed: !cacheEnabled }">
        <div class="card-head">
          <div>
            <div class="card-title">使用默认缓存条件</div>
            <div class="card-desc">对齐 GoEdge：选用一套全局条件包；本站规则优先级更高。</div>
          </div>
          <div class="card-extra">
            <NuxtLink to="/cache-policies" class="btn btn-default btn-sm">管理条件包</NuxtLink>
          </div>
        </div>
        <div class="card-body form-col">
          <label class="switch" :class="{ on: useDefault, disabled: saving || !cacheEnabled }">
            <span class="switch-track" />
            <input
              v-model="useDefault"
              type="checkbox"
              style="display: none"
              :disabled="saving || !cacheEnabled"
              @change="onUseDefaultChange"
            />
            使用默认缓存条件
          </label>

          <div v-if="useDefault" class="field">
            <label class="field-label">条件包</label>
            <div class="policy-row">
              <select
                v-model="selectedPolicyId"
                class="select"
                :disabled="saving || !cacheEnabled"
                @change="savePolicyBind"
              >
                <option v-for="p in policies" :key="p.id" :value="p.id">
                  {{ p.name }}{{ p.is_builtin ? '（内置）' : '' }}
                </option>
              </select>
            </div>
            <span v-if="selectedPolicy?.note" class="field-hint">{{ selectedPolicy.note }}</span>
          </div>

          <div v-if="useDefault && policyRules.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>路径</th>
                  <th>动作</th>
                  <th>TTL</th>
                  <th>优先级</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="r in policyRules" :key="r.id">
                  <td class="mono">{{ r.path_pattern }}</td>
                  <td>
                    <StatusTag :tone="actionTone(r.action)">{{ actionLabel(r.action) }}</StatusTag>
                  </td>
                  <td class="num">{{ r.action === 'cache' ? `${r.ttl_seconds} 秒` : '—' }}</td>
                  <td class="num">{{ r.priority }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <p v-else-if="useDefault" class="text-muted" style="font-size: 13px">该条件包暂无条目。</p>
        </div>
      </div>

      <div class="card" :class="{ dimmed: !cacheEnabled }">
        <div class="card-head">
          <div>
            <div class="card-title">本站缓存规则</div>
            <div class="card-desc">仅作用于本站，优先级高于默认缓存条件。</div>
          </div>
          <div class="card-extra">
            <NuxtLink to="/rules" class="btn btn-default btn-sm">跨站总览</NuxtLink>
            <button class="btn btn-primary btn-sm" type="button" :disabled="!cacheEnabled" @click="openCreate">
              新建规则
            </button>
          </div>
        </div>
        <div v-if="siteRules.length" class="table-wrap">
          <table class="table">
            <thead>
              <tr>
                <th>路径匹配</th>
                <th>动作</th>
                <th>TTL</th>
                <th>SWR</th>
                <th>优先级</th>
                <th class="text-right">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in siteRules" :key="r.id">
                <td class="mono">{{ r.path_pattern }}</td>
                <td>
                  <StatusTag :tone="actionTone(r.action)">{{ actionLabel(r.action) }}</StatusTag>
                </td>
                <td class="num">{{ r.action === 'cache' ? `${r.ttl_seconds} 秒` : '—' }}</td>
                <td class="num">{{ r.swr_seconds ? `${r.swr_seconds} 秒` : '—' }}</td>
                <td class="num">{{ r.priority }}</td>
                <td class="text-right">
                  <button class="btn btn-text btn-sm" type="button" @click="openEdit(r)">编辑</button>
                  <button class="btn btn-text btn-sm danger" type="button" :disabled="busy" @click="removeRule(r)">
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
          title="暂无本站专属规则"
          text="未配置本站规则时，仅按上方默认缓存条件与源站缓存头处理。"
        >
          <button class="btn btn-default btn-sm" type="button" :disabled="!cacheEnabled" @click="openCreate">
            新建本站规则
          </button>
        </EmptyState>
      </div>
    </template>

    <BaseModal
      v-model="formOpen"
      :title="editing ? '编辑本站规则' : '新建本站规则'"
      confirm-text="保存"
      :loading="saving"
      :confirm-disabled="!form.path_pattern.trim()"
      width="560px"
      @confirm="submitRule"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">路径 <span class="req">*</span></label>
          <input v-model="form.path_pattern" class="input mono" placeholder="/static/* 或正则" />
        </div>
        <div class="field-row">
          <div class="field">
            <label class="field-label">匹配方式</label>
            <select v-model="form.match_type" class="select">
              <option value="glob">通配</option>
              <option value="regex">正则</option>
            </select>
          </div>
          <div class="field">
            <label class="field-label">动作</label>
            <select v-model="form.action" class="select">
              <option value="cache">缓存</option>
              <option value="bypass">绕过</option>
              <option value="no_store">不存储</option>
            </select>
          </div>
          <div class="field">
            <label class="field-label">优先级</label>
            <input v-model.number="form.priority" class="input" type="number" />
          </div>
        </div>
        <div v-if="form.action === 'cache'" class="field-row">
          <div class="field">
            <label class="field-label">边缘 TTL（秒）</label>
            <input v-model.number="form.ttl_seconds" class="input" type="number" min="1" />
          </div>
          <div class="field">
            <label class="field-label">SWR（秒）</label>
            <input v-model.number="form.swr_seconds" class="input" type="number" min="0" />
          </div>
          <div class="field">
            <label class="field-label">浏览器 TTL</label>
            <input v-model.number="form.browser_ttl_seconds" class="input" type="number" min="0" />
          </div>
        </div>
        <label class="check">
          <input v-model="form.ignore_cookie" type="checkbox" />
          忽略 Cookie（适合静态资源）
        </label>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="form.note" class="input" />
        </div>
      </div>
    </BaseModal>
  </div>
</template>

<style scoped>
.site-cache {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.dimmed {
  opacity: 0.55;
  pointer-events: none;
}
.form-col {
  display: flex;
  flex-direction: column;
  gap: 12px;
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
.policy-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.policy-row .select {
  flex: 1;
  min-width: 200px;
}
.card-extra {
  display: flex;
  gap: 8px;
  align-items: center;
}
.check {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
</style>
