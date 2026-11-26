<template>
  <div class="page">
    <div class="page-head">
      <div>
        <div class="page-title">IP 名单</div>
        <div class="page-desc">可复用的名单供站点引用；管理员也可创建自动应用到所有站点的全局黑名单。</div>
      </div>
      <button class="btn btn-primary" type="button" @click="openCreate">新建名单</button>
    </div>

    <div class="card">
      <div class="card-body">
        <div v-if="loading && !lists.length" class="text-muted">正在加载名单…</div>
        <template v-else>
          <div v-if="lists.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>名称</th>
                  <th>类型</th>
                  <th>范围</th>
                  <th>条目</th>
                  <th>状态</th>
                  <th style="width: 180px">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="l in lists" :key="l.id">
                  <td>{{ l.name }}</td>
                  <td>{{ l.list_type === 'white' ? '白名单' : '黑名单' }}</td>
                  <td>{{ l.applies_globally ? '全站自动' : l.project_id ? '项目内' : '手动绑定' }}</td>
                  <td>{{ l.entry_count }}</td>
                  <td>{{ l.enabled ? '启用' : '停用' }}</td>
                  <td>
                    <button class="btn btn-text btn-sm" type="button" @click="openEntries(l)">管理条目</button>
                    <button class="btn btn-text btn-sm danger" type="button" :disabled="busy" @click="removeList(l)">删除</button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <EmptyState v-else icon="shield" title="还没有 IP 名单" text="创建后可在站点 WAF 中引用。" />
        </template>
      </div>
    </div>

    <BaseModal
      v-model="createOpen"
      title="新建 IP 名单"
      confirm-text="创建"
      :loading="saving"
      :confirm-disabled="!form.name.trim()"
      @confirm="createList"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">名称 <span class="req">*</span></label>
          <input v-model="form.name" class="input" placeholder="办公网出口" />
        </div>
        <div class="field">
          <label class="field-label">类型</label>
          <select v-model="form.list_type" class="select">
            <option value="black">黑名单</option>
            <option value="white">白名单</option>
          </select>
        </div>
        <div v-if="auth.isAdmin && form.list_type === 'black'" class="field">
          <label class="check-row">
            <input v-model="form.applies_globally" type="checkbox" />
            <span>
              <strong>全局黑名单</strong>
              <small>自动应用到所有站点，优先于站点白名单，站点 WAF 关闭时仍会拦截。</small>
            </span>
          </label>
        </div>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="form.note" class="input" />
        </div>
      </div>
    </BaseModal>

    <BaseModal
      v-model="entryOpen"
      :title="`条目：${activeList?.name || ''}`"
      confirm-text="添加"
      :loading="saving"
      :confirm-disabled="!entryForm.cidr.trim()"
      width="640px"
      @confirm="addEntry"
    >
      <div class="form-col">
        <div class="field-row">
          <div class="field" style="flex: 1">
            <label class="field-label">IP / CIDR <span class="req">*</span></label>
            <input v-model="entryForm.cidr" class="input mono" placeholder="10.0.0.0/8" />
          </div>
          <div class="field" style="flex: 1">
            <label class="field-label">备注</label>
            <input v-model="entryForm.note" class="input" />
          </div>
        </div>
        <div v-if="entries.length" class="table-wrap">
          <table class="table">
            <thead>
              <tr>
                <th>CIDR</th>
                <th>备注</th>
                <th style="width: 80px">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="e in entries" :key="e.id">
                <td class="mono">{{ e.cidr }}</td>
                <td>{{ e.note || '—' }}</td>
                <td>
                  <button class="btn btn-text btn-sm danger" type="button" :disabled="busy" @click="removeEntry(e)">删除</button>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </BaseModal>
  </div>
</template>

<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'

const api = useApi()
const toast = useToast()
const { ask } = useConfirm()
const auth = useAuthStore()

const lists = ref<any[]>([])
const entries = ref<any[]>([])
const activeList = ref<any>(null)
const createOpen = ref(false)
const entryOpen = ref(false)
const saving = ref(false)
const loading = ref(false)
const busy = ref(false)
const form = reactive({ name: '', list_type: 'black', note: '', applies_globally: false })
const entryForm = reactive({ cidr: '', note: '' })

async function load() {
  loading.value = true
  try {
    lists.value = (await api.get<any[]>('/api/v1/ip-lists')) || []
  } catch (e: any) {
    lists.value = []
    toast.error(friendlyErrorMessage(e?.data?.error, '加载名单失败'))
  } finally {
    loading.value = false
  }
}

function openCreate() {
  form.name = ''
  form.list_type = 'black'
  form.note = ''
  form.applies_globally = false
  createOpen.value = true
}

watch(() => form.list_type, (value) => {
  if (value !== 'black') form.applies_globally = false
})

async function createList() {
  if (!form.name.trim()) {
    toast.error('请填写名单名称')
    return
  }
  saving.value = true
  try {
    await api.post('/api/v1/ip-lists', { ...form, name: form.name.trim() })
    toast.success('已创建')
    createOpen.value = false
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '创建失败'))
  } finally {
    saving.value = false
  }
}

async function openEntries(l: any) {
  activeList.value = l
  try {
    entries.value = (await api.get<any[]>(`/api/v1/ip-lists/${l.id}/entries`)) || []
  } catch (e: any) {
    entries.value = []
    toast.error(friendlyErrorMessage(e?.data?.error, '加载条目失败'))
  }
  entryForm.cidr = ''
  entryForm.note = ''
  entryOpen.value = true
}

async function addEntry() {
  if (!activeList.value) return
  if (!entryForm.cidr.trim()) {
    toast.error('请填写 IP 或 CIDR')
    return
  }
  saving.value = true
  try {
    await api.post(`/api/v1/ip-lists/${activeList.value.id}/entries`, {
      cidr: entryForm.cidr.trim(),
      note: entryForm.note.trim(),
    })
    toast.success('已添加')
    entryForm.cidr = ''
    entryForm.note = ''
    entries.value =
      (await api.get<any[]>(`/api/v1/ip-lists/${activeList.value.id}/entries`).catch(() => [])) || []
    await load()
  } catch (e: any) {
    toast.error(friendlyErrorMessage(e?.data?.error, '添加失败'))
  } finally {
    saving.value = false
  }
}

async function removeEntry(e: any) {
  if (!activeList.value || busy.value) return
  const ok = await ask({
    title: '删除条目',
    message: `确认删除「${e.cidr}」？`,
    danger: true,
    confirmText: '删除',
  })
  if (!ok) return
  busy.value = true
  try {
    await api.del(`/api/v1/ip-lists/${activeList.value.id}/entries/${e.id}`)
    entries.value = entries.value.filter((x) => x.id !== e.id)
    toast.success('条目已删除')
    await load()
  } catch (err: any) {
    toast.error(friendlyErrorMessage(err?.data?.error, '删除失败'))
  } finally {
    busy.value = false
  }
}

async function removeList(l: any) {
  if (busy.value) return
  if (!(await ask({ title: '删除名单', message: `确认删除「${l.name}」？`, danger: true, confirmText: '删除' })))
    return
  busy.value = true
  try {
    await api.del(`/api/v1/ip-lists/${l.id}`)
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
.check-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  color: var(--text-1);
}

.check-row input {
  margin-top: 3px;
}

.check-row span {
  display: grid;
  gap: 4px;
}

.check-row small {
  color: var(--text-3);
  line-height: 1.5;
}
</style>
