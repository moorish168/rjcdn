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
  quickBanEventIp,
  quickBanBusy,
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
  eventHasMore,
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

function changePage(page: number) {
  eventPage.value = Math.max(1, page)
  loadEvents()
}
</script>

<template>
  <div class="card-body tight">

          <div class="filter-bar">
            <select v-model="eventAction" class="select">
              <option value="">全部动作</option>
              <option value="block">拦截</option>
              <option value="challenge">安全验证</option>
              <option value="monitor">观察</option>
              <option value="log">仅记录</option>
            </select>
            <select v-model.number="eventLimit" class="select">
              <option :value="50">50 条 / 页</option>
              <option :value="100">100 条 / 页</option>
              <option :value="200">200 条 / 页</option>
              <option :value="500">500 条 / 页（接口上限）</option>
            </select>
            <button class="btn btn-default" type="button" :disabled="eventsLoading" @click="loadEvents">
              查询
            </button>
            <span class="spacer" />
            <span class="text-muted" style="font-size: 13px">第 {{ eventPage }} 页 · 本页 {{ events.length }} 条</span>
          </div>

          <div v-if="filteredEvents.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>时间</th>
                  <th>动作</th>
                  <th>类型</th>
                  <th>规则</th>
                  <th>客户端 IP</th>
                  <th style="width: 105px">操作</th>
                  <th>方法</th>
                  <th style="min-width: 200px">路径</th>
                  <th>节点</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="e in pagedEvents" :key="e.id">
                  <td class="text-muted nowrap">{{ formatDateTime(e.ts) }}</td>
                  <td>
                    <StatusTag :tone="eventTone(e.action)">{{ eventLabel(e.action) }}</StatusTag>
                  </td>
                  <td>{{ categoryLabel(e.category) }}</td>
                  <td class="text-2">{{ e.rule_name || '—' }}</td>
                  <td class="mono ip-cell" :title="e.client_ip">{{ e.client_ip }}</td>
                  <td>
                    <button
                      v-if="auth.isAdmin || auth.can('waf:write')"
                      class="btn btn-text btn-sm danger"
                      type="button"
                      :disabled="quickBanBusy !== null || !e.client_ip"
                      @click="quickBanEventIp(e)"
                    >
                      {{ quickBanBusy === e.id ? '处理中…' : '拉黑本站' }}
                    </button>
                    <span v-else class="text-muted">—</span>
                  </td>
                  <td class="mono">{{ e.method }}</td>
                  <td>
                    <div class="mono path-cell" :title="e.path">{{ e.path }}</div>
                  </td>
                  <td class="text-2">{{ e.node_id }}</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else-if="eventsLoading" class="card-body">
            <div class="skeleton" style="height: 32px; width: 100%" />
          </div>
          <EmptyState v-else icon="activity" title="暂无 WAF 事件" text="命中规则的请求会在这里展示；「仅记录」模式下只记日志、不拦截。" />
          <div v-if="events.length || eventPage > 1" class="table-footer event-pagination">
            <span class="text-muted">第 {{ eventPage }} 页 · {{ eventHasMore ? '还有后续记录' : '已到末页' }}</span>
            <div class="page-actions">
              <button
                class="btn btn-default btn-sm"
                type="button"
                :disabled="eventPage <= 1 || eventsLoading"
                @click="changePage(eventPage - 1)"
              >
                上一页
              </button>
              <button
                class="btn btn-default btn-sm"
                type="button"
                :disabled="!eventHasMore || eventsLoading"
                @click="changePage(eventPage + 1)"
              >
                下一页
              </button>
            </div>
          </div>
        
  </div>
</template>

<style scoped>
.ip-cell {
  min-width: 160px;
  overflow-wrap: anywhere;
  white-space: normal;
}

.event-pagination {
  justify-content: space-between;
}
</style>
