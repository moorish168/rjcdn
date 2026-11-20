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
  refreshSyncStatus,
  runPublicCheck,
  nodeNames,
  syncRows,
  syncLoaded,
  syncSerial,
  syncStateLabels,
  publicCheck,
  publicChecking,
  publicCheckError,
  checkStateLabels,
  verdictTones,
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
  recordPlaceholder,
} = dns
</script>

<template>
  <div class="dns-section">

        <div class="card">
          <div class="card-head">
            <div>
              <div class="card-title">CDN 主域</div>
              <div class="card-desc">平台首次接入时配置；站点 CNAME 会指向这里。边缘节点上线后会自动写入主域 A/AAAA（离线暂停、删除清除），解析仍走 GeoDNS。</div>
            </div>
          </div>
          <div class="card-body">
            <div v-if="loading" class="skeleton" style="height: 180px" />
            <div v-else class="form-col">
              <div class="field">
                <label class="field-label">CDN 主域 <span class="req">*</span></label>
                <input
                  v-model="form.zone"
                  class="input mono"
                  placeholder="edge.example.com"
                  @blur="autofillNs"
                />
                <span class="field-hint">例如 edge.example.com。站点走 CNAME 接入时指向这个名字。</span>
              </div>
              <div class="field-row">
                <div class="field">
                  <label class="field-label">缓存时间（秒）</label>
                  <input v-model.number="form.ttl" class="input" type="number" min="5" max="300" />
                </div>
                <div class="field">
                  <label class="field-label">兜底 IP</label>
                  <input v-model="form.fallback_ip" class="input mono" placeholder="节点不可用时返回的 IP" />
                </div>
              </div>
              <label class="check-row">
                <input v-model="form.enabled" type="checkbox" />
                <span>启用 CDN 主域</span>
              </label>

              <div class="field" style="margin-top: 8px">
                <div class="flex-between" style="margin-bottom: 8px">
                  <label class="field-label" style="margin: 0">平台 DNS 服务器</label>
                  <button class="btn btn-default btn-sm" type="button" @click="addNs">添加服务器</button>
                </div>
                <div class="table-wrap">
                  <table class="table">
                    <thead>
                      <tr>
                        <th>服务器名</th>
                        <th>解析 IP</th>
                        <th style="width: 80px" />
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="(row, idx) in form.ns" :key="idx">
                        <td>
                          <input v-model="row.name" class="input mono" placeholder="ns1.edge.example.com" />
                        </td>
                        <td>
                          <input v-model="row.ip" class="input mono" placeholder="203.0.113.53" />
                        </td>
                        <td>
                          <button
                            class="btn btn-text btn-sm danger"
                            type="button"
                            :disabled="form.ns.length <= 1"
                            @click="removeNs(idx)"
                          >
                            删除
                          </button>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
                <span class="field-hint">
                  与注册商处填写的 DNS 服务器 / glue（DNS Host）保持一致。
                </span>
              </div>

              <div class="flex-between" style="margin-top: 8px">
                <p v-if="error" class="text-danger" style="font-size: 13px; margin: 0">{{ error }}</p>
                <span v-else />
                <button class="btn btn-primary" type="button" :disabled="saving" @click="saveSettings">
                  {{ saving ? '保存中…' : '保存 CDN 主域' }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <div class="card" style="margin-top: 16px">
          <div class="card-head">
            <div class="card-title">DNS 节点同步</div>
            <button class="btn btn-default btn-sm" type="button" @click="refreshSyncStatus">刷新</button>
          </div>
          <div class="card-body">
            <div v-if="!syncLoaded" class="skeleton" style="height: 80px" />
            <div v-else-if="syncRows.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>节点</th>
                    <th>公网 IP</th>
                    <th>配置</th>
                    <th>最近上报</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="n in syncRows" :key="n.id">
                    <td>
                      <div v-for="nm in nodeNames(n)" :key="nm">{{ nm }}</div>
                    </td>
                    <td class="mono">{{ n.public_ip }}</td>
                    <td>
                      <StatusTag :tone="syncStateLabels[n.sync].tone" dot>{{ syncStateLabels[n.sync].text }}</StatusTag>
                      <span v-if="n.sync === 'syncing'" class="text-muted mono sync-serial">
                        {{ n.last_serial }} → {{ syncSerial }}
                      </span>
                    </td>
                    <td class="text-muted">{{ formatTime(n.last_seen_at) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <EmptyState
              v-else
              icon="node"
              title="还没有 DNS 节点"
              text="DNS 服务启动后会自动出现在这里；保存上面的 DNS 服务器也会登记对应节点。"
            />
            <p v-if="syncRows.some((r) => r.sync === 'unreported')" class="field-hint mt-12">
              「未上报」的节点没有开启心跳，看不出用的是哪一版配置：在它的 /etc/lagran/dns.env 加上
              <span class="mono">DNS_PUBLIC_IP=本机公网 IP</span> 后重启 lagran-dns，或重新运行 install.sh dns。
            </p>
            <p v-if="syncRows.some((r) => r.sync === 'offline')" class="field-hint mt-12">
              「离线」的节点在服务器上用 <span class="mono">systemctl status lagran-dns</span> 查看是否在运行。
            </p>
          </div>
        </div>

        <div class="card" style="margin-top: 16px">
          <div class="card-head">
            <div class="card-title">外部检测</div>
            <button
              class="btn btn-default btn-sm"
              type="button"
              :disabled="publicChecking || !form.zone.trim()"
              @click="runPublicCheck"
            >
              {{ publicChecking ? '检测中…' : '开始检测' }}
            </button>
          </div>
          <div class="card-body">
            <p class="text-muted check-intro">
              通过 Google、Cloudflare、阿里 DNS 查询 CDN 主域，看公网能不能解析到平台：注册商的 DNS 服务器没改、glue 填错、安全组没放行 53 端口，都能在这里看出来。
            </p>
            <p v-if="publicCheckError" class="text-danger" style="font-size: 13px; margin: 0">{{ publicCheckError }}</p>
            <template v-if="publicCheck">
              <div class="check-summary">
                <StatusTag :tone="verdictTones[publicCheck.verdict]" dot>
                  {{ { ok: '已生效', partial: '部分生效', fail: '未生效', unknown: '无法检测' }[publicCheck.verdict] }}
                </StatusTag>
                <span>{{ publicCheck.summary }}</span>
              </div>
              <div class="table-wrap">
                <table class="table">
                  <thead>
                    <tr>
                      <th>公共 DNS</th>
                      <th>结果</th>
                      <th>看到的 DNS 服务器</th>
                      <th>测试子域</th>
                      <th>说明</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="r in publicCheck.resolvers" :key="r.key">
                      <td>{{ r.name }}</td>
                      <td>
                        <StatusTag :tone="checkStateLabels[r.state].tone">{{ checkStateLabels[r.state].text }}</StatusTag>
                      </td>
                      <td class="mono">{{ r.ns.length ? r.ns.join(', ') : '—' }}</td>
                      <td class="mono">{{ r.sample_ip || '—' }}</td>
                      <td class="check-detail">
                        {{ r.detail }}
                        <span v-if="r.comment" class="text-muted mono check-comment">{{ r.comment }}</span>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <p class="text-muted check-intro" style="margin: 8px 0 0">
                测试子域 <span class="mono">{{ publicCheck.probe_name }}</span> · {{ formatTime(publicCheck.checked_at) }}
              </p>
            </template>
          </div>
        </div>

  </div>
</template>
