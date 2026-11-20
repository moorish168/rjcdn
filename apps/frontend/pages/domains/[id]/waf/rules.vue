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
              按优先级从高到低匹配，命中即停止；支持拦截、放行、仅记录三种动作
            </span>
            <span class="spacer" />
            <button class="btn btn-primary btn-sm" type="button" @click="openRuleModal()">
              <AppIcon name="plus" :size="14" />
              新建规则
            </button>
          </div>

          <div v-if="loading" class="text-muted" style="padding: 24px 0">加载规则中…</div>
          <template v-else>
            <div v-if="rules.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>规则名称</th>
                    <th>匹配条件</th>
                    <th>动作</th>
                    <th>优先级</th>
                    <th>状态</th>
                    <th class="text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="r in pagedRules" :key="r.id">
                    <td class="cell-main">{{ r.name }}</td>
                    <td>
                      <span v-if="r.expr" class="mono dsl-cell" :title="r.expr">
                        <span class="dsl-badge">DSL</span>{{ r.expr }}
                      </span>
                      <span v-else class="mono">{{ fieldLabel(r.field) }} {{ opLabel(r.operator) }} {{ r.value }}</span>
                    </td>
                    <td>
                      <StatusTag :tone="actionTone(r.action)">{{ actionLabel(r.action) }}</StatusTag>
                    </td>
                    <td class="num">{{ r.priority }}</td>
                    <td>
                      <StatusTag :tone="r.enabled ? 'success' : 'neutral'" dot>
                        {{ r.enabled ? '启用' : '停用' }}
                      </StatusTag>
                    </td>
                    <td class="text-right">
                      <div class="ops" style="justify-content: flex-end">
                        <button class="btn btn-text btn-sm" type="button" @click="toggleRule(r)">
                          {{ r.enabled ? '停用' : '启用' }}
                        </button>
                        <button class="btn btn-text btn-sm" type="button" @click="openRuleModal(r)">
                          编辑
                        </button>
                        <button class="btn btn-text btn-sm danger" type="button" @click="removeRule(r)">
                          删除
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div v-if="rules.length > rulePageSize" class="table-footer">
              <AppPagination v-model:page="rulePage" :total="rules.length" :page-size="rulePageSize" />
            </div>
            <EmptyState v-if="!rules.length" icon="rules" title="还没有自定义规则"
              text="例如：路径前缀 /admin 且来源不属于办公网时拦截，或对 /api/ 只记录日志。">
              <button class="btn btn-primary btn-sm" type="button" @click="openRuleModal()">
                新建规则
              </button>
            </EmptyState>
          </template>
        
  </div>
</template>
