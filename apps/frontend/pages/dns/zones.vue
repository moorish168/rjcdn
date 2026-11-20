<script setup lang="ts">
const dns = inject('dns') as ReturnType<typeof useDnsConsole>
const {
  dnsActionLabel,
  dnsOpStatusLabel,
  dnsOpTone,
  autofillNs,
  addNs,
  removeNs,
  loadSettings,
  saveSettings,
  loadZones,
  openZones,
  selectZone,
  loadRecords,
  openCreateZone,
  createZone,
  removeZone,
  loadLines,
  openLines,
  openCreateLine,
  openEditLine,
  saveLine,
  openCreateRecord,
  openEditRecord,
  saveRecord,
  removeRecord,
  openNodes,
  loadNodes,
  openRemote,
  ensureRemoteSsh,
  saveRemoteSsh,
  runDnsRemote,
  openDnsOpLog,
  openCreateNode,
  createNode,
  removeNode,
  runNodeTest,
  runTest,
  formatTime,
  reload,
  route,
  api,
  auth,
  toast,
  section,
  lines,
  lineOptions,
  lineOpen,
  lineSaving,
  editingLine,
  lineForm,
  recordLineLabel,
  loading,
  saving,
  error,
  form,
  zonesLoading,
  zones,
  selectedZone,
  recordsLoading,
  records,
  nodesLoading,
  dnsNodes,
  remoteDnsMap,
  sshGrants,
  nodeOpen,
  nodeSaving,
  nodeError,
  nodeForm,
  remoteOpen,
  remoteSaving,
  remoteBusy,
  remoteError,
  remoteNode,
  remoteForm,
  dnsRemoteActions,
  dnsOpLogOpen,
  dnsOpLogText,
  testLoading,
  testError,
  testResult,
  testForm,
  dnsNodeRows,
  zoneOpen,
  zoneSaving,
  zoneError,
  zoneForm,
  recordOpen,
  recordSaving,
  recordError,
  editingRecord,
  recordTypes,
  recordForm,
  nsHostnames,
  delegationLines,
  recordPlaceholder,
} = dns

const recordPage = ref(1)
const recordPageSize = ref(20)
const recordKeyword = ref('')
const filteredRecords = computed(() => {
  const q = recordKeyword.value.trim().toLowerCase()
  if (!q) return records.value
  return records.value.filter((r) =>
    [
      r.name,
      r.rtype,
      r.cdn_proxied ? r.origin : r.content,
      recordLineLabel(r),
      r.site_name,
      r.cdn_proxied ? '加速' : r.managed_by_site_id || r.site_id ? '仅DNS' : '',
    ]
      .map((v) => String(v || '').toLowerCase())
      .some((v) => v.includes(q)),
  )
})
const pagedRecords = computed(() => {
  const start = (recordPage.value - 1) * recordPageSize.value
  return filteredRecords.value.slice(start, start + recordPageSize.value)
})
watch(
  () => [selectedZone.value?.id, records.value.length, recordKeyword.value, recordPageSize.value],
  () => {
    recordPage.value = 1
  },
)
</script>

<template>
  <div class="dns-section">

        <div class="card">
          <div class="card-head">
            <div>
              <div class="card-title">域名列表</div>
              <div class="card-desc" style="margin-top: 4px">
                由平台解析的域名；站点选择「整域托管」时也会自动加到这里。
              </div>
            </div>
            <button class="btn btn-primary btn-sm" type="button" @click="openCreateZone">添加域名</button>
          </div>
          <div class="card-body">
            <div v-if="zonesLoading" class="skeleton" style="height: 120px" />
            <div v-else-if="zones.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>域名</th>
                    <th>类型</th>
                    <th>关联站点</th>
                    <th>状态</th>
                    <th style="width: 200px">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="z in zones"
                    :key="z.id"
                    :class="{ active: selectedZone?.id === z.id }"
                    @click="selectZone(z)"
                  >
                    <td class="mono">{{ z.name }}</td>
                    <td>{{ z.kind === 'schedule' ? 'CDN 主域' : '托管' }}</td>
                    <td>
                      <NuxtLink
                        v-if="z.site_id"
                        :to="`/domains/${z.site_id}`"
                        class="btn btn-text btn-sm"
                        @click.stop
                      >
                        {{ z.site_name || '查看站点' }}
                      </NuxtLink>
                      <span v-else-if="z.kind === 'schedule'" class="text-muted">—</span>
                      <span v-else class="text-muted">未关联站点</span>
                    </td>
                    <td>
                      <StatusTag :tone="z.enabled ? 'success' : 'neutral'" dot>
                        {{ z.enabled ? '启用' : '停用' }}
                      </StatusTag>
                    </td>
                    <td>
                      <button class="btn btn-text btn-sm" type="button" @click.stop="selectZone(z)">
                        记录
                      </button>
                      <button
                        v-if="z.kind !== 'schedule'"
                        class="btn btn-text btn-sm danger"
                        type="button"
                        @click.stop="removeZone(z)"
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
              icon="globe"
              title="还没有域名"
              text="添加域名后，把它在注册商处的 DNS 服务器改成平台的服务器名，就能在这里管理解析记录。"
            >
              <button class="btn btn-primary btn-sm" type="button" @click="openCreateZone">添加域名</button>
            </EmptyState>
          </div>
        </div>

        <div v-if="selectedZone" class="card" style="margin-top: 16px">
          <div class="card-head">
            <div class="card-title">
              记录 · <span class="mono">{{ selectedZone.name }}</span>
            </div>
            <button class="btn btn-primary btn-sm" type="button" @click="openCreateRecord">添加记录</button>
          </div>
          <div class="card-body">
            <div v-if="records.length" class="filter-bar" style="margin-bottom: 12px">
              <input
                v-model="recordKeyword"
                class="input"
                placeholder="筛选主机名 / 类型 / 值"
                style="min-width: 220px"
              />
              <span class="spacer" />
              <span class="text-muted" style="font-size: 13px">{{ filteredRecords.length }} 条</span>
            </div>
            <div v-if="recordsLoading" class="skeleton" style="height: 120px" />
            <div v-else-if="filteredRecords.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>主机名</th>
                    <th>类型</th>
                    <th>值</th>
                    <th>TTL</th>
                    <th>地区</th>
                    <th>加速状态</th>
                    <th style="width: 140px">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="r in pagedRecords" :key="r.id">
                    <td class="mono">{{ r.name }}</td>
                    <td>{{ r.rtype }}{{ r.rtype === 'MX' && r.priority != null ? ` ${r.priority}` : '' }}</td>
                    <td class="mono">
                      <!-- Edge inventory: show the node's public IP (GeoDNS still answers). -->
                      <template v-if="r.managed_by_node_id">
                        {{ r.content }}
                        <span v-if="!r.enabled" class="text-muted">（已暂停）</span>
                      </template>
                      <!-- Orange cloud: placeholder content; show origin host only. -->
                      <span
                        v-else-if="r.cdn_proxied"
                        :title="r.origin ? `访客解析到 CDN，边缘回源到 ${r.origin}` : '访客解析到 CDN 节点'"
                      >
                        {{ r.origin || '—' }}
                      </span>
                      <!-- Grey cloud: content is already the origin target. -->
                      <span
                        v-else-if="r.managed_by_site_id || r.site_id"
                        :title="'CDN 已关闭，访客 DNS 直达源站'"
                      >
                        {{ r.content }}
                      </span>
                      <template v-else>{{ r.content }}</template>
                    </td>
                    <td>{{ r.ttl }}</td>
                    <td style="white-space: nowrap" :title="r.line_name || ''">{{ recordLineLabel(r) }}</td>
                    <td>
                      <template v-if="r.managed_by_node_id">
                        <NuxtLink
                          :to="`/nodes/${r.managed_by_node_id}/detail`"
                          :title="`由边缘节点「${r.managed_node_name || ''}」自动维护`"
                        >
                          <StatusTag :tone="r.enabled ? 'success' : 'neutral'">
                            边缘节点{{ r.managed_node_name ? ` · ${r.managed_node_name}` : '' }}
                          </StatusTag>
                        </NuxtLink>
                      </template>
                      <template v-else-if="r.cdn_proxied">
                        <NuxtLink
                          v-if="r.site_id"
                          :to="`/domains/${r.site_id}/settings`"
                          title="由站点「整域托管」自动维护，可在站点里一键关闭加速"
                        >
                          <StatusTag tone="info">站点加速</StatusTag>
                        </NuxtLink>
                        <StatusTag v-else tone="info" title="由站点整域托管自动维护">站点加速</StatusTag>
                      </template>
                      <template v-else-if="r.managed_by_site_id || r.site_id">
                        <NuxtLink
                          v-if="r.site_id"
                          :to="`/domains/${r.site_id}/settings`"
                          title="CDN 已关闭，解析直回源站；可在站点里重新开启加速"
                        >
                          <StatusTag tone="neutral">仅 DNS</StatusTag>
                        </NuxtLink>
                        <StatusTag v-else tone="neutral">仅 DNS</StatusTag>
                      </template>
                      <span v-else class="text-muted">—</span>
                    </td>
                    <td>
                      <button
                        v-if="!r.cdn_proxied && !r.managed_by_node_id && !r.managed_by_site_id"
                        class="btn btn-text btn-sm"
                        type="button"
                        @click="openEditRecord(r)"
                      >
                        编辑
                      </button>
                      <button
                        v-if="!r.managed_by_node_id"
                        class="btn btn-text btn-sm danger"
                        type="button"
                        @click="removeRecord(r)"
                      >
                        删除
                      </button>
                      <span v-else class="text-muted" style="font-size: 12px">自动维护</span>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <EmptyState
              v-else-if="records.length"
              icon="search"
              title="没有匹配的记录"
              text="试试其他关键词，或清空筛选。"
            />
            <EmptyState
              v-else
              icon="globe"
              title="还没有记录"
              text="点「添加记录」添加解析记录；整域托管站点的加速记录由站点自动写入。"
            />
            <div v-if="filteredRecords.length > recordPageSize" class="table-footer">
              <AppPagination
                v-model:page="recordPage"
                v-model:page-size="recordPageSize"
                :total="filteredRecords.length"
                :page-size="recordPageSize"
                :page-sizes="[10, 20, 50]"
              />
            </div>
          </div>
        </div>
      
  </div>
</template>
