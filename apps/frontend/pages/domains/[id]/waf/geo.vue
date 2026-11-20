<script setup lang="ts">
const waf = inject('waf') as ReturnType<typeof useWafConsole>
const {
  actionCount,
  barHeight,
  loadStats,
  fieldLabel,
  opLabel,
  actionLabel,
  actionTone,
  eventLabel,
  eventTone,
  categoryLabel,
  loadDomains,
  loadSettings,
  loadRulesets,
  slotLabel,
  formatRulepackTime,
  loadRulepacks,
  publishRulepack,
  retireRulepack,
  deleteRulepack,
  rollbackRulepack,
  toggleRuleset,
  loadRules,
  loadIps,
  openBindListModal,
  confirmBindList,
  unbindIpList,
  geoKindLabel,
  geoKindTone,
  geoValueLabel,
  loadGeo,
  loadEvents,
  loadBans,
  openBanModal,
  saveBan,
  banStatusLabel,
  banTone,
  remainingLabel,
  unbanIp,
  reload,
  enterSection,
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
  toggleRule,
  removeRule,
  openIpModal,
  saveIp,
  toggleIp,
  removeIp,
  openGeoModal,
  saveGeo,
  toggleGeo,
  removeGeo,
  api,
  toast,
  auth,
  route,
  domains,
  domainId,
  section,
  loading,
  saving,
  stats,
  statsLoading,
  securityLevels,
  levelHint,
  settings,
  rules,
  rulesets,
  rulepackVersions,
  rulepackBusy,
  ips,
  ipBindings,
  availableIpLists,
  bindListOpen,
  bindListId,
  bindSaving,
  geoRules,
  geoPage,
  geoPageSize,
  pagedGeo,
  rulePage,
  ipPage,
  rulePageSize,
  ipPageSize,
  pagedRules,
  pagedIps,
  events,
  eventsLoading,
  eventAction,
  eventLimit,
  bans,
  bansLoading,
  bansError,
  banStatusFilter,
  currentDomainName,
  filteredBans,
  ccStats,
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
  managedRules,
  tabs,
  maxDayCount,
  filteredEvents,
  eventPage,
  eventPageSize,
  pagedEvents,
  dslTextarea,
  dslSuggestOpen,
  dslSuggestions,
  dslSuggestIndex,
  dslTokenStart,
  DSL_FIELDS,
  DSL_OPS,
  DSL_KEYWORDS,
} = waf
</script>

<template>
  <div class="card-body tight">

          <div class="filter-bar">
            <span class="text-muted" style="font-size: 13px">
              按洲、国家/地区控制访问。黑名单优先；配置白名单后，只有所选地区的访客能进入后续防护检查
            </span>
            <span class="spacer" />
            <button class="btn btn-primary btn-sm" type="button" @click="openGeoModal('deny')">
              <AppIcon name="plus" :size="14" />
              添加地区规则
            </button>
          </div>

          <div v-if="loading" class="text-muted" style="padding: 24px 0">加载地区名单中…</div>
          <template v-else>
            <div v-if="geoRules.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>类型</th>
                    <th>范围</th>
                    <th>备注</th>
                    <th>添加时间</th>
                    <th>状态</th>
                    <th class="text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="rule in pagedGeo" :key="rule.id">
                    <td>
                      <StatusTag :tone="geoKindTone(rule.kind)">{{ geoKindLabel(rule.kind) }}</StatusTag>
                    </td>
                    <td>
                      <div class="geo-scope" :title="geoValueLabel(rule, 300)">
                        {{ geoValueLabel(rule) }}
                      </div>
                    </td>
                    <td class="text-2">{{ rule.note || '—' }}</td>
                    <td class="text-muted">{{ formatDateTime(rule.created_at) }}</td>
                    <td>
                      <StatusTag :tone="rule.enabled ? 'success' : 'neutral'" dot>
                        {{ rule.enabled ? '启用' : '停用' }}
                      </StatusTag>
                    </td>
                    <td class="text-right">
                      <div class="ops" style="justify-content: flex-end">
                        <button class="btn btn-text btn-sm" type="button" @click="toggleGeo(rule)">
                          {{ rule.enabled ? '停用' : '启用' }}
                        </button>
                        <button class="btn btn-text btn-sm" type="button" @click="openGeoModal('deny', rule)">
                          编辑
                        </button>
                        <button class="btn btn-text btn-sm danger" type="button" @click="removeGeo(rule)">
                          删除
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div v-if="geoRules.length > geoPageSize" class="table-footer">
              <AppPagination v-model:page="geoPage" :total="geoRules.length" :page-size="geoPageSize" />
            </div>
            <EmptyState v-if="!geoRules.length" icon="shield" title="还没有地区名单"
              text="例如：只允许中国大陆访问，或一次勾选多个洲、国家封禁高风险地区。">
              <button class="btn btn-primary btn-sm" type="button" @click="openGeoModal('deny')">
                添加第一条
              </button>
            </EmptyState>
          </template>
        
  </div>
</template>

<style scoped>
.geo-scope {
  max-width: 420px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text);
}
</style>
