<script setup lang="ts">
import { displayVersion } from '~/utils/version'
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
  nodeNames,
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
            <div class="card-title">DNS 节点</div>
            <div class="flex gap-8">
              <NuxtLink to="/ssh-grants" class="btn btn-default btn-sm">SSH 凭证</NuxtLink>
              <button class="btn btn-primary btn-sm" type="button" @click="openCreateNode">添加节点</button>
            </div>
          </div>
          <div class="card-body">
            <p class="text-muted" style="font-size: 12px; margin: 0 0 12px">
              保存 CDN 主域的 DNS 服务器后会自动出现在这里；控制面会定期检查节点是否在线。远程安装使用全局 SSH 凭证。
              多个服务器名指向同一个 IP 时是同一台 DNS 服务器，显示在同一行。
            </p>
            <div v-if="nodesLoading" class="skeleton" style="height: 120px" />
            <div v-else-if="dnsNodes.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>名称</th>
                    <th>公网 IP</th>
                    <th>状态</th>
                    <th>SSH</th>
                    <th>版本</th>
                    <th>最近任务</th>
                    <th style="width: 220px">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="n in dnsNodeRows" :key="n.id">
                    <td>
                      <div v-for="nm in nodeNames(n)" :key="nm">{{ nm }}</div>
                    </td>
                    <td class="mono">{{ n.public_ip }}</td>
                    <td>
                      <StatusTag :tone="n.healthy ? 'success' : 'neutral'" dot>
                        {{ n.healthy ? '在线' : n.status }}
                      </StatusTag>
                    </td>
                    <td class="mono">
                      <template v-if="n.ssh_host">{{ n.ssh_host }}:{{ n.ssh_port }}</template>
                      <span v-else class="text-muted">未配置</span>
                    </td>
                    <td class="mono">{{ displayVersion(n.version) || '—' }}</td>
                    <td>
                      <button
                        v-if="n.last_op"
                        class="btn btn-text btn-sm"
                        type="button"
                        @click="openDnsOpLog(n.last_op.id)"
                      >
                        {{ dnsActionLabel(n.last_op.action) }}
                        <StatusTag :tone="dnsOpTone(n.last_op.status)">
                          {{ dnsOpStatusLabel(n.last_op.status) }}
                        </StatusTag>
                      </button>
                      <span v-else class="text-muted">—</span>
                    </td>
                    <td>
                      <button class="btn btn-text btn-sm" type="button" @click="openRemote(n)">
                        远程安装
                      </button>
                      <button class="btn btn-text btn-sm" type="button" @click="runNodeTest(n)">
                        测试
                      </button>
                      <button class="btn btn-text btn-sm danger" type="button" @click="removeNode(n)">
                        删除
                      </button>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <EmptyState
              v-else
              icon="node"
              title="还没有 DNS 节点"
              text="先在「CDN 主域」保存 DNS 服务器，或点右上角手动添加。"
            />
          </div>
        </div>

        <div class="card" style="margin-top: 16px">
          <div class="card-head">
            <div class="card-title">DNS 测试</div>
            <button class="btn btn-primary btn-sm" type="button" :disabled="testLoading" @click="runTest">
              {{ testLoading ? '查询中…' : '查询' }}
            </button>
          </div>
          <div class="card-body form-col">
            <div class="field-row">
              <div class="field">
                <label class="field-label">查询名</label>
                <input v-model="testForm.name" class="input mono" placeholder="cdn.edge.example.com" />
              </div>
              <div class="field">
                <label class="field-label">目标 DNS</label>
                <input
                  v-model="testForm.server"
                  class="input mono"
                  placeholder="203.0.113.53:53（空则用 DNS_TEST_SERVER）"
                />
              </div>
            </div>
            <div class="field">
              <label class="field-label">模拟客户端 IP / 子网（ECS）</label>
              <input v-model="testForm.ip" class="input mono" placeholder="可选，如 203.0.113.9/24" />
            </div>
            <div v-if="testResult" class="guide-pre">
              server={{ testResult.server }} · rcode={{ testResult.rcode }} ·
              {{ testResult.elapsed_ms }}ms
              <template v-if="testResult.answers?.length">
                · {{ testResult.answers.map((a: any) => a.ip).join(', ') }}
              </template>
            </div>
            <p v-if="testError" class="text-danger" style="font-size: 13px">{{ testError }}</p>
          </div>
        </div>
      
  </div>
</template>
