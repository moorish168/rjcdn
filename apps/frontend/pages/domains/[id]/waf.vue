<script setup lang="ts">
/** Site-scoped WAF console (GoEdge: 网站 → 设置 → WAF). */
const siteId = inject<Ref<string>>('domainId')!
const waf = useWafConsole({ domainId: siteId })
provide('waf', waf)

const {
  reload,
  saveSettings,
  openRuleModal,
  dslFieldKeys,
  dslOpKeys,
  insideDslString,
  updateDslSuggestions,
  closeDslSuggestionsSoon,
  applyDslSuggestion,
  onDslKeydown,
  saveRule,
  openIpModal,
  saveIp,
  openGeoModal,
  saveGeo,
  openBanModal,
  saveBan,
  openBindListModal,
  confirmBindList,
  section,
  loading,
  saving,
  settings,
  ipBindings,
  availableIpLists,
  bindListOpen,
  bindListId,
  bindSaving,
  ruleModalOpen,
  editingRule,
  ruleError,
  ruleForm,
  ipModalOpen,
  ipError,
  editingIp,
  ipForm,
  geoModalOpen,
  geoError,
  editingGeo,
  geoForm,
  banModalOpen,
  banError,
  banForm,
  tabs,
  dslTextarea,
  dslSuggestOpen,
  dslSuggestions,
  dslSuggestIndex,
  DSL_FIELDS,
  DSL_OPS,
  DSL_KEYWORDS,
} = waf

function sectionTo(value: string) {
  return `/domains/${siteId.value}/waf/${value}`
}
</script>

<template>
  <div class="waf-console">
    <div class="card waf-toolbar">
      <div class="flex-center gap-12" style="flex-wrap: wrap">
        <div class="card-title" style="margin: 0">WAF 防护</div>
        <StatusTag :tone="settings.enabled ? 'success' : 'neutral'" dot>
          {{ settings.enabled ? '防护已开启' : '防护未开启' }}
        </StatusTag>
        <StatusTag :tone="settings.mode === 'block' ? 'danger' : 'warning'">
          {{ settings.mode === 'block' ? '拦截中' : '仅记录' }}
        </StatusTag>
        <span class="spacer" />
        <span class="text-muted" style="font-size: 12px">规则变更后边缘约 15 秒内同步</span>
        <button class="btn btn-default btn-sm" type="button" :disabled="loading" @click="reload">
          <AppIcon name="refresh" :size="14" />
          刷新
        </button>
      </div>
    </div>

    <div class="card">
      <div class="waf-tabs">
        <NuxtLink
          v-for="t in tabs"
          :key="t.value"
          :to="sectionTo(t.value)"
          class="waf-tab"
          :class="{ active: section === t.value }"
        >
          {{ t.label }}
          <span v-if="t.count !== undefined" class="tab-count">{{ t.count }}</span>
        </NuxtLink>
      </div>
      <NuxtPage />
    </div>

    <BaseModal v-model="ruleModalOpen" :title="editingRule ? '编辑 WAF 规则' : '新建 WAF 规则'" confirm-text="保存"
      :loading="saving" @confirm="saveRule">
      <div class="form-col">
        <div class="field">
          <label class="field-label">规则名称 <span class="req">*</span></label>
          <input v-model="ruleForm.name" class="input" placeholder="例如：拦截后台探测" />
        </div>
        <div class="form-grid">
          <div class="field">
            <label class="field-label">匹配字段</label>
            <select v-model="ruleForm.field" class="select" :disabled="!!ruleForm.expr.trim()">
              <option value="path">路径 Path</option>
              <option value="query">查询串 Query</option>
              <option value="user_agent">User-Agent</option>
              <option value="referer">Referer</option>
              <option value="method">请求方法</option>
              <option value="ip">客户端 IP</option>
              <option value="header">请求头（自定义）</option>
            </select>
          </div>
          <div v-if="ruleForm.field === 'header'" class="field">
            <label class="field-label">请求头名称 <span class="req">*</span></label>
            <input v-model="ruleForm.header_name" class="input" placeholder="例如 x-api-key"
              :disabled="!!ruleForm.expr.trim()" />
          </div>
          <div class="field">
            <label class="field-label">匹配方式</label>
            <select v-model="ruleForm.operator" class="select" :disabled="!!ruleForm.expr.trim()">
              <option value="contains">包含</option>
              <option value="equals">等于</option>
              <option value="prefix">前缀</option>
              <option value="suffix">后缀</option>
            </select>
          </div>
          <div class="field span-2">
            <label class="field-label">匹配内容 <span v-if="!ruleForm.expr.trim()" class="req">*</span></label>
            <input v-model="ruleForm.value" class="input" placeholder="例如 /admin 或 sqlmap"
              :disabled="!!ruleForm.expr.trim()" />
          </div>
          <div class="field">
            <label class="field-label">命中动作</label>
            <select v-model="ruleForm.action" class="select">
              <option value="block">拦截</option>
              <option value="challenge">安全验证</option>
              <option value="allow">放行</option>
              <option value="log">仅记录</option>
            </select>
          </div>
          <div class="field">
            <label class="field-label">优先级</label>
            <input v-model.number="ruleForm.priority" class="input" type="number" />
          </div>
        </div>
        <div class="field">
          <label class="field-label">高级条件 DSL（可选）</label>
          <div class="dsl-editor">
            <textarea ref="dslTextarea" v-model="ruleForm.expr" class="textarea mono" rows="4"
              placeholder='例如：path prefix "/admin" AND ip in "10.0.0.0/8"' @input="updateDslSuggestions"
              @focus="updateDslSuggestions" @click="updateDslSuggestions" @keydown="onDslKeydown"
              @blur="closeDslSuggestionsSoon" />
            <div v-if="dslSuggestOpen && dslSuggestions.length" class="dsl-suggest">
              <button v-for="(item, i) in dslSuggestions" :key="item.label" type="button" class="dsl-suggest-item"
                :class="{ active: i === dslSuggestIndex }" @mousedown.prevent="applyDslSuggestion(item)">
                <span class="dsl-suggest-label mono">{{ item.label }}</span>
                <span class="dsl-suggest-detail">{{ item.detail }}</span>
              </button>
              <div class="dsl-suggest-foot">↑↓ 选择 · Enter/Tab 补全 · Esc 关闭</div>
            </div>
          </div>
          <span class="field-hint">
            DSL 非空时忽略上面的单条件。支持 path/query/user_agent/referer/method/ip/header:名称、
            contains/equals/prefix/suffix、ip in "CIDR"、AND/OR/NOT 和括号；不允许正则。
          </span>
        </div>
        <label class="check">
          <input v-model="ruleForm.enabled" type="checkbox" />
          启用这条规则
        </label>
        <p v-if="ruleError" class="text-danger" style="font-size: 13px">{{ ruleError }}</p>
      </div>
    </BaseModal>

    <BaseModal v-model="ipModalOpen" :title="editingIp ? '编辑 IP 名单' : '添加 IP / 网段'"
      :confirm-text="editingIp ? '保存' : '添加'" :loading="saving" @confirm="saveIp">
      <div class="form-col">
        <div class="field">
          <label class="field-label">名单类型</label>
          <div class="segmented">
            <button type="button" :class="{ active: ipForm.kind === 'deny' }" @click="ipForm.kind = 'deny'">
              黑名单
            </button>
            <button type="button" :class="{ active: ipForm.kind === 'allow' }" @click="ipForm.kind = 'allow'">
              白名单
            </button>
          </div>
        </div>
        <div class="field">
          <label class="field-label">IP 或 CIDR <span class="req">*</span></label>
          <input v-model="ipForm.ip" class="input" placeholder="203.0.113.10 或 10.0.0.0/8" />
        </div>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="ipForm.note" class="input" placeholder="例如：攻击来源 / 办公网出口" />
        </div>
        <label class="check">
          <input v-model="ipForm.enabled" type="checkbox" />
          启用这条规则
        </label>
        <p v-if="ipError" class="text-danger" style="font-size: 13px">{{ ipError }}</p>
      </div>
    </BaseModal>

    <BaseModal
      v-model="bindListOpen"
      title="引用可复用 IP 名单"
      confirm-text="引用"
      :loading="bindSaving"
      @confirm="confirmBindList"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">选择名单</label>
          <select v-model="bindListId" class="select">
            <option disabled value="">请选择</option>
            <option v-for="l in availableIpLists" :key="l.id" :value="l.id">
              {{ l.name }}（{{ l.list_type === 'white' ? '白' : '黑' }} · {{ l.entry_count }} 条）
            </option>
          </select>
          <span class="field-hint">
            名单在「IP 名单」中维护；引用后下发到边缘的 allow/deny 会合并名单条目。
            <NuxtLink to="/ip-lists">去管理</NuxtLink>
          </span>
        </div>
        <p v-if="!availableIpLists.length" class="text-muted" style="font-size: 13px">
          没有可引用的名单，请先到 IP 名单页创建。
        </p>
      </div>
    </BaseModal>

    <BaseModal v-model="geoModalOpen" :title="editingGeo ? '编辑地区规则' : '添加地区规则'"
      :confirm-text="editingGeo ? '保存' : '添加'" :loading="saving" width="720px" @confirm="saveGeo">
      <div class="form-col">
        <div class="field">
          <label class="field-label">名单类型</label>
          <div class="segmented">
            <button type="button" :class="{ active: geoForm.action === 'deny' }" @click="geoForm.action = 'deny'">
              黑名单
            </button>
            <button type="button" :class="{ active: geoForm.action === 'allow' }" @click="geoForm.action = 'allow'">
              白名单
            </button>
          </div>
        </div>
        <div class="field">
          <label class="field-label">
            {{ geoForm.action === 'deny' ? '禁止访问的地区' : '允许访问的地区' }}
            <span class="req">*</span>
          </label>
          <span class="field-hint">
            {{ geoForm.action === 'deny'
              ? '来自所选地区的请求直接拦截。勾选洲会选中该洲全部国家/地区，可再取消个别。'
              : '配置白名单后，只有所选地区的访客能继续访问，其他地区一律拦截。' }}
          </span>
          <GeoAreaPicker v-model="geoForm.countries" />
        </div>
        <div class="field">
          <label class="field-label">备注</label>
          <input v-model="geoForm.note" class="input" placeholder="例如：仅允许国内访问 / 封禁高风险地区" />
        </div>
        <label class="check">
          <input v-model="geoForm.enabled" type="checkbox" />
          启用这条规则
        </label>
        <p v-if="geoError" class="text-danger" style="font-size: 13px">{{ geoError }}</p>
      </div>
    </BaseModal>

    <BaseModal v-model="banModalOpen" title="添加共享封禁" confirm-text="封禁" :loading="saving" @confirm="saveBan">
      <div class="form-col">
        <div class="field">
          <label class="field-label">域名</label>
          <div class="input" style="display: flex; align-items: center; background: var(--bg)">
            {{ currentDomainName || '请先选择防护域名' }}
          </div>
        </div>
        <div class="field">
          <label class="field-label">IP 或 CIDR <span class="req">*</span></label>
          <input v-model="banForm.ip" class="input" placeholder="203.0.113.10 或 203.0.113.0/24" />
          <span class="field-hint">支持 IPv4/IPv6 单 IP 与 CIDR；边缘按域名 + 网段匹配。</span>
        </div>
        <div class="form-grid">
          <div class="field">
            <label class="field-label">封禁时长（秒）</label>
            <input v-model.number="banForm.seconds" class="input" type="number" min="1" max="86400" />
          </div>
          <div class="field">
            <label class="field-label">原因</label>
            <input v-model="banForm.reason" class="input" placeholder="例如：持续 CC 攻击" />
          </div>
        </div>
        <p v-if="banError" class="text-danger" style="font-size: 13px">{{ banError }}</p>
      </div>
    </BaseModal>
  </div>
</template>

<style>
.waf-console .cc-stat-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--divider);
  background: var(--bg);
}
.waf-console .cc-stat {
  color: var(--text-3);
  font-size: 12px;
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--card);
}
.waf-console .cc-stat b {
  color: var(--text);
  margin-right: 3px;
}
.waf-console .waf-toolbar {
  padding: 14px 18px;
  margin-bottom: 16px;
}
.waf-console .waf-tabs {
  display: flex;
  gap: 4px;
  padding: 0 18px;
  border-bottom: 1px solid var(--divider);
  overflow-x: auto;
}
.waf-console .waf-tab {
  position: relative;
  border: none;
  background: transparent;
  padding: 14px 12px 12px;
  font-size: 14px;
  color: var(--text-2);
  cursor: pointer;
  border-bottom: 2px solid transparent;
  white-space: nowrap;
}
.waf-console .waf-tab:hover {
  color: var(--brand);
}
.waf-console .waf-tab.active {
  color: var(--brand);
  font-weight: 500;
  border-bottom-color: var(--brand);
}
.waf-console .tab-count {
  margin-left: 4px;
  font-size: 11px;
  color: var(--text-3);
}
.waf-console .bar-chart {
  display: flex;
  align-items: flex-end;
  gap: 6px;
  height: 150px;
  padding: 12px 4px 0;
  border-bottom: 1px solid var(--divider);
}
.waf-console .bar-col {
  flex: 1;
  min-width: 8px;
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  align-items: center;
  gap: 6px;
}
.waf-console .bar {
  width: 100%;
  max-width: 28px;
  background: linear-gradient(180deg, #3b82f6, #2563eb);
  border-radius: 3px 3px 0 0;
  transition: height 0.2s ease;
}
.waf-console .bar-label {
  font-size: 10px;
  color: var(--text-3);
  white-space: nowrap;
}
.waf-console .waf-stat-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
  margin-top: 20px;
}
.waf-console .stat-list {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px 14px;
  min-height: 120px;
}
.waf-console .stat-list-title {
  font-size: 12px;
  color: var(--text-3);
  margin-bottom: 8px;
}
.waf-console .stat-list-row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 0;
  font-size: 12.5px;
  color: var(--text-2);
}
.waf-console .stat-list-row b {
  font-weight: 500;
  color: var(--text);
  font-variant-numeric: tabular-nums;
}
.waf-console .stat-empty {
  font-size: 12px;
  color: var(--text-4);
}
.waf-console .truncate {
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.waf-console .level-seg {
  flex-wrap: wrap;
}
@media (max-width: 900px) {
.waf-console .waf-stat-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }


}
.waf-console .setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  padding: 14px 0;
  border-bottom: 1px solid var(--divider);
}
.waf-console .setting-title {
  font-size: 14px;
  color: var(--text);
}
.waf-console .setting-desc {
  margin-top: 2px;
  font-size: 12px;
  color: var(--text-3);
}
.waf-console .section-label {
  margin: 20px 0 10px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-2);
}
.waf-console .managed-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.waf-console .managed-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  cursor: pointer;
  transition: all 0.15s ease;
}
.waf-console .managed-item.on {
  border-color: #bedaff;
  background: #f7faff;
}
.waf-console .managed-item input {
  margin-top: 3px;
  accent-color: var(--brand);
}
.waf-console .managed-info {
  flex: 1;
  min-width: 0;
}
.waf-console .managed-info strong {
  display: block;
  font-size: 13.5px;
  font-weight: 500;
  color: var(--text);
}
.waf-console .managed-info small {
  display: block;
  margin-top: 2px;
  font-size: 12px;
  color: var(--text-3);
  line-height: 1.5;
}
.waf-console .managed-item .app-icon {
  color: var(--text-4);
}
.waf-console .managed-item.on .app-icon {
  color: var(--brand);
}
.waf-console /* managed rulesets */
.ruleset-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}
.waf-console .ruleset-card {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--card);
  cursor: pointer;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}
.waf-console .ruleset-card:hover {
  border-color: var(--border-strong);
}
.waf-console .ruleset-card.on {
  border-color: var(--brand-bg-strong);
  background: var(--brand-bg);
  box-shadow: 0 4px 14px rgba(37, 99, 235, 0.06);
}
.waf-console .ruleset-card input {
  margin-top: 3px;
  accent-color: var(--brand);
}
.waf-console .ruleset-body {
  flex: 1;
  min-width: 0;
}
.waf-console .ruleset-head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
}
.waf-console .ruleset-head strong {
  font-size: 13.5px;
  color: var(--text);
}
.waf-console .ruleset-version {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text-3);
}
.waf-console .ruleset-badge {
  font-size: 11px;
  line-height: 18px;
  padding: 0 7px;
  border-radius: 9px;
  color: var(--brand);
  background: var(--brand-bg);
}
.waf-console .ruleset-body small {
  display: block;
  margin-top: 5px;
  font-size: 12px;
  line-height: 1.55;
  color: var(--text-3);
}
.waf-console .ruleset-categories {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin-top: 9px;
}
.waf-console .ruleset-chip {
  font-size: 11px;
  line-height: 20px;
  padding: 0 7px;
  border-radius: 10px;
  background: var(--bg);
  color: var(--text-2);
}
.waf-console .ruleset-card .app-icon {
  color: var(--text-4);
}
.waf-console .ruleset-card.on .app-icon {
  color: var(--brand);
}
.waf-console /* rulepack version / rollout */
.rulepack-admin {
  margin-top: 14px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg);
}
.waf-console .rulepack-versions {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.waf-console .rulepack-version-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 11px 12px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--card);
  transition: border-color 0.15s ease, background 0.15s ease;
}
.waf-console .rulepack-version-row:hover {
  border-color: var(--border-strong);
  background: var(--bg);
}
.waf-console .rulepack-version-row.slot-active {
  border-color: var(--brand-bg-strong);
  background: var(--brand-bg);
}
.waf-console .version-main {
  flex: 1;
  min-width: 0;
}
.waf-console .version-title {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}
.waf-console .version-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}
.waf-console .slot-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 20px;
  padding: 0 8px;
  border-radius: 10px;
  font-size: 11px;
  line-height: 20px;
}
.waf-console .slot-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
}
.waf-console .slot-badge-active {
  color: var(--success);
  background: var(--success-bg);
}
.waf-console .slot-badge-previous {
  color: var(--warning);
  background: var(--warning-bg);
}
.waf-console .slot-badge-retired {
  color: var(--text-3);
  background: var(--bg);
}
.waf-console .version-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
  font-size: 11.5px;
  color: var(--text-3);
}
.waf-console .meta-sep {
  color: var(--text-4);
}
.waf-console .version-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 6px;
  flex: none;
}
.waf-console .version-action {
  min-width: 82px;
}
.waf-console .current-label {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--success);
  font-size: 12px;
  white-space: nowrap;
}
.waf-console /* custom rule DSL */
.dsl-editor {
  position: relative;
}
.waf-console .dsl-suggest {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 40;
  max-height: 220px;
  overflow-y: auto;
  background: var(--card);
  border: 1px solid var(--border-strong);
  border-radius: 9px;
  box-shadow: var(--shadow);
  padding: 4px;
}
.waf-console .dsl-suggest-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 9px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.waf-console .dsl-suggest-item:hover, .waf-console .dsl-suggest-item.active {
  background: var(--brand-bg);
  color: var(--brand);
}
.waf-console .dsl-suggest-label {
  font-weight: 600;
  color: var(--text);
}
.waf-console .dsl-suggest-item.active .dsl-suggest-label {
  color: var(--brand);
}
.waf-console .dsl-suggest-detail {
  margin-left: auto;
  font-size: 11px;
  color: var(--text-3);
}
.waf-console .dsl-suggest-foot {
  padding: 6px 9px 3px;
  font-size: 10.5px;
  color: var(--text-3);
  border-top: 1px solid var(--divider);
  margin-top: 3px;
}
.waf-console .dsl-cell {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 420px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11.5px;
}
.waf-console .dsl-badge {
  flex: none;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--brand-bg);
  color: var(--brand);
  font-family: var(--font);
  font-size: 10px;
  line-height: 18px;
}
@media (max-width: 900px) {
.waf-console .managed-grid, .waf-console .ruleset-grid {
    grid-template-columns: minmax(0, 1fr);
  }
.waf-console .rulepack-version-row {
    align-items: flex-start;
    flex-direction: column;
    gap: 10px;
  }
.waf-console .version-actions {
    width: 100%;
    justify-content: flex-start;
  }


}
</style>
