<script setup lang="ts">
const dns = useDnsConsole()
provide('dns', dns)

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

const DNS_TABS = [
  { value: 'zones', label: '域名解析' },
  { value: 'providers', label: 'DNS 服务商' },
  { value: 'nodes', label: 'DNS 节点' },
  { value: 'lines', label: '自定义线路' },
  { value: 'settings', label: 'CDN 主域' },
] as const

function sectionTo(value: string) {
  return `/dns/${value}`
}
</script>

<template>
  <div class="page dns-console">
    <div class="page-head">
      <div>
        <div class="page-title">智能 DNS</div>
        <div class="page-desc">日常维护托管域名与解析；CDN 主域只在平台首次接入时配置一次</div>
      </div>
      <div v-if="auth.isAdmin" class="page-actions">
        <button class="btn btn-default" type="button" :disabled="loading" @click="reload">
          <AppIcon name="refresh" :size="15" />
          刷新
        </button>
      </div>
    </div>

    <div v-if="!auth.isAdmin" class="alert alert-warning">
      <AppIcon name="shield" :size="16" class="icon" />
      <span>只有管理员可以配置智能 DNS。</span>
    </div>

    <template v-else>
      <div class="tabs">
        <NuxtLink
          v-for="t in DNS_TABS"
          :key="t.value"
          :to="sectionTo(t.value)"
          class="tab"
          :class="{ active: section === t.value }"
        >
          {{ t.label }}
        </NuxtLink>
      </div>

      <NuxtPage />
    </template>

    <BaseModal
      v-model="zoneOpen"
      title="添加域名"
      confirm-text="添加"
      :loading="zoneSaving"
      @confirm="createZone"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">域名 <span class="req">*</span></label>
          <input v-model="zoneForm.name" class="input mono" placeholder="example.com" />
          <span class="field-hint">
            添加后，到注册商处把该域名的 DNS 服务器改成平台的服务器名（见「CDN 主域」），解析记录即在这里生效。
          </span>
        </div>
        <p v-if="zoneError" class="text-danger" style="font-size: 13px">{{ zoneError }}</p>
      </div>
    </BaseModal>

    <BaseModal
      v-model="nodeOpen"
      title="添加 DNS 节点"
      confirm-text="添加"
      :loading="nodeSaving"
      @confirm="createNode"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">名称 <span class="req">*</span></label>
          <input v-model="nodeForm.name" class="input" placeholder="ns1-hk" />
        </div>
        <div class="field">
          <label class="field-label">公网 IP <span class="req">*</span></label>
          <input v-model="nodeForm.public_ip" class="input mono" placeholder="203.0.113.53" />
        </div>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="nodeForm.notes" class="input" placeholder="可选" />
        </div>
        <p v-if="nodeError" class="text-danger" style="font-size: 13px">{{ nodeError }}</p>
      </div>
    </BaseModal>

    <BaseModal
      v-model="remoteOpen"
      :title="`远程安装：${remoteNode?.name || ''}`"
      confirm-text="保存 SSH"
      :loading="remoteSaving"
      width="560px"
      @confirm="saveRemoteSsh"
    >
      <div class="form-col">
        <p class="text-muted" style="font-size: 12px; margin: 0">
          SSH 凭证为全局凭证，与边缘节点共用。
          <NuxtLink to="/ssh-grants">管理 SSH 凭证</NuxtLink>
        </p>
        <div class="field-row">
          <div class="field">
            <label class="field-label">SSH 地址 <span class="req">*</span></label>
            <input v-model="remoteForm.host" class="input mono" placeholder="公网 IP 或域名" />
          </div>
          <div class="field" style="max-width: 120px">
            <label class="field-label">端口</label>
            <input v-model.number="remoteForm.port" class="input mono" type="number" min="1" max="65535" />
          </div>
        </div>
        <div class="field">
          <label class="field-label">SSH 凭证 <span class="req">*</span></label>
          <select v-model="remoteForm.grant_id" class="select">
            <option value="" disabled>请选择</option>
            <option v-for="g in sshGrants" :key="g.id" :value="g.id">
              {{ g.name }}（{{ g.username }}）
            </option>
          </select>
        </div>
        <div class="flex gap-8" style="flex-wrap: wrap">
          <button
            v-for="a in dnsRemoteActions"
            :key="a.key"
            class="btn btn-sm"
            :class="a.danger ? 'btn-danger' : 'btn-default'"
            type="button"
            :disabled="remoteBusy || !remoteForm.grant_id || !remoteForm.host"
            @click="runDnsRemote(a.key)"
          >
            {{ a.label }}
          </button>
        </div>
        <p v-if="remoteError" class="text-danger" style="font-size: 13px">{{ remoteError }}</p>
      </div>
    </BaseModal>

    <BaseModal v-model="dnsOpLogOpen" title="任务日志" hide-footer width="720px">
      <pre class="op-log">{{ dnsOpLogText || '加载中…' }}</pre>
    </BaseModal>

    <BaseModal
      v-model="lineOpen"
      :title="editingLine ? '编辑自定义线路' : '添加自定义线路'"
      confirm-text="保存"
      :loading="lineSaving"
      width="720px"
      @confirm="saveLine"
    >
      <div class="form-col">
        <div class="field-row">
          <div class="field">
            <label class="field-label">Code</label>
            <input v-model="lineForm.code" class="input mono" placeholder="na-us" :disabled="!!editingLine" />
          </div>
          <div class="field">
            <label class="field-label">名称</label>
            <input v-model="lineForm.name" class="input" placeholder="北美 / 美国" />
          </div>
        </div>
        <div class="field">
          <label class="field-label">匹配地区（洲 → 国家/地区）</label>
          <span class="field-hint">勾选洲会选中该洲全部国家/地区，可再取消个别。也可叠加省/运营商做更细匹配。</span>
          <GeoAreaPicker v-model="lineForm.countries" />
        </div>
        <div class="field-row">
          <div class="field">
            <label class="field-label">省份（可选）</label>
            <input v-model="lineForm.province" class="input" placeholder="广东" />
          </div>
          <div class="field">
            <label class="field-label">运营商（可选）</label>
            <input v-model="lineForm.isp" class="input" placeholder="电信" />
          </div>
        </div>
        <label class="check-row"><input v-model="lineForm.is_default" type="checkbox" /> 设为默认</label>
        <label class="check-row"><input v-model="lineForm.enabled" type="checkbox" /> 启用</label>
      </div>
    </BaseModal>

    <BaseModal
      v-model="recordOpen"
      :title="editingRecord ? '编辑记录' : '添加记录'"
      confirm-text="保存"
      :loading="recordSaving"
      :confirm-disabled="!recordForm.content.trim()"
      width="760px"
      @confirm="saveRecord"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">主机名</label>
          <input v-model="recordForm.name" class="input mono" placeholder="@ 或 www" />
        </div>
        <div class="field-row">
          <div class="field">
            <label class="field-label">类型</label>
            <select v-model="recordForm.rtype" class="select">
              <option v-for="t in recordTypes" :key="t" :value="t">{{ t }}</option>
            </select>
          </div>
          <div class="field">
            <label class="field-label">TTL（秒）</label>
            <input v-model.number="recordForm.ttl" class="input" type="number" min="5" max="86400" />
          </div>
        </div>
        <div v-if="recordForm.rtype === 'MX'" class="field">
          <label class="field-label">优先级</label>
          <input v-model.number="recordForm.priority" class="input" type="number" min="0" max="65535" />
        </div>
        <div class="field">
          <label class="field-label">值 <span class="req">*</span></label>
          <input
            v-model="recordForm.content"
            class="input mono"
            :placeholder="recordPlaceholder"
          />
        </div>
        <div v-if="['A', 'AAAA', 'CNAME'].includes(recordForm.rtype)" class="field">
          <label class="field-label">解析请求来源 <span class="req">*</span></label>
          <span class="field-hint">
            按访客来源返回本条记录，与阿里云「默认 / 地域 / 运营商 / 自定义线路」一致；选「默认」即全部兜底。
          </span>
          <DnsLineSourcePicker
            v-model:countries="recordForm.countries"
            v-model:line-id="recordForm.line_id"
            :lines="lines"
          />
        </div>
        <p v-if="recordError" class="text-danger" style="font-size: 13px">{{ recordError }}</p>
      </div>
    </BaseModal>
  </div>
</template>

<style>
.dns-console .tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}
.dns-console .tab {
  border: 1px solid var(--border, #ddd);
  background: transparent;
  border-radius: 8px;
  padding: 6px 14px;
  cursor: pointer;
  font-size: 13px;
}
.dns-console .tab.active {
  background: var(--bg-muted, #f3f3f3);
  font-weight: 600;
}
.dns-console .field-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.dns-console .guide {
  display: flex;
  flex-direction: column;
  gap: 14px;
  font-size: 13px;
  line-height: 1.55;
}
.dns-console .guide-block strong {
  display: block;
  margin-bottom: 4px;
}
.dns-console .guide-pre {
  margin: 8px 0 0;
  padding: 10px 12px;
  border-radius: 8px;
  background: var(--bg-muted, #f5f5f5);
  font-size: 12px;
  white-space: pre-wrap;
}
.dns-console tr.active {
  background: color-mix(in srgb, var(--bg-muted, #f5f5f5) 80%, transparent);
}
.dns-console .sync-serial {
  margin-left: 6px;
  font-size: 12px;
}
.dns-console .check-intro {
  font-size: 12px;
  margin: 0 0 12px;
}
.dns-console .check-summary {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin-bottom: 12px;
  font-size: 13px;
  line-height: 1.55;
}
.dns-console .check-summary > :first-child {
  flex-shrink: 0;
}
.dns-console .check-detail {
  font-size: 12px;
  line-height: 1.5;
  min-width: 260px;
}
.dns-console .check-comment {
  display: block;
  margin-top: 2px;
  font-size: 11px;
  word-break: break-all;
}
.dns-console .op-log {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: calc(100vh - 260px);
  overflow: auto;
  margin: 0;
}
@media (max-width: 720px) {
.dns-console .field-row {
    grid-template-columns: 1fr;
  }


}
</style>
