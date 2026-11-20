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
  linesLoading,
  lineOptions,
  lineOpen,
  lineSaving,
  editingLine,
  lineForm,
  lineMatchLabel,
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
</script>

<template>
  <div class="dns-section">

        <div class="card">
          <div class="card-head">
            <div class="card-title">自定义线路</div>
            <button class="btn btn-primary btn-sm" type="button" @click="openCreateLine">添加自定义线路</button>
          </div>
          <div class="card-body">
            <p class="text-muted" style="font-size: 12px; margin-top: 0">
              添加解析时勾选洲/国家即可自动建线。多条自定义同时命中时更细粒度优先；都未命中（含 IP 库未匹配）走「默认线路」，等同全部兜底。
            </p>
            <div v-if="linesLoading" class="skeleton" style="height: 120px" />
            <div v-else-if="lines.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>名称</th>
                    <th>Code</th>
                    <th>匹配</th>
                    <th>默认</th>
                    <th>状态</th>
                    <th style="width: 120px">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="l in lines" :key="l.id">
                    <td>{{ l.name }}</td>
                    <td class="mono">{{ l.code }}</td>
                    <td class="mono" style="font-size: 12px">
                      {{ lineMatchLabel(l) }}
                    </td>
                    <td>{{ l.is_default ? '是' : '—' }}</td>
                    <td>{{ l.enabled ? '启用' : '停用' }}</td>
                    <td>
                      <button class="btn btn-text btn-sm" type="button" @click="openEditLine(l)">编辑</button>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <EmptyState v-else icon="globe" title="暂无自定义线路" text="系统仅保留默认线路作全部兜底；添加解析时勾选地区会自动生成线路。" />
          </div>
        </div>
      
  </div>
</template>
