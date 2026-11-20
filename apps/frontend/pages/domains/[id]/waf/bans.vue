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

const banPage = ref(1)
const banPageSize = ref(20)
const pagedBans = computed(() => {
  const start = (banPage.value - 1) * banPageSize.value
  return filteredBans.value.slice(start, start + banPageSize.value)
})
watch([banStatusFilter, () => filteredBans.value.length, banPageSize], () => {
  banPage.value = 1
})
</script>

<template>
  <div class="card-body tight">

          <div class="filter-bar">
            <select v-model="banStatusFilter" class="select">
              <option value="">全部状态</option>
              <option value="active">生效中</option>
              <option value="expired">已过期</option>
              <option value="revoked">已解封</option>
            </select>
            <span class="text-muted" style="font-size: 13px">
              当前域名 {{ currentDomainName || '—' }}；当前封禁会同步到所有边缘，过期与已解封记录保留用于审计
            </span>
            <span class="spacer" />
            <button class="btn btn-default btn-sm" type="button" :disabled="bansLoading" @click="loadBans">
              <AppIcon name="refresh" :size="14" />
              刷新
            </button>
            <button class="btn btn-primary btn-sm" type="button" @click="openBanModal">
              <AppIcon name="plus" :size="14" />
              添加封禁
            </button>
            <span class="text-muted" style="font-size: 13px">{{ filteredBans.length }} 条记录</span>
          </div>

          <div v-if="ccStats" class="cc-stat-row">
            <span class="cc-stat"><b>{{ ccStats.active_bans }}</b> 活动封禁</span>
            <span class="cc-stat"><b>{{ ccStats.limited_24h }}</b> 24h 限流</span>
            <span class="cc-stat"><b>{{ ccStats.challenged_24h }}</b> 24h 挑战</span>
            <span class="cc-stat"><b>{{ ccStats.banned_24h }}</b> 24h 封禁</span>
          </div>

          <div v-if="bansError" class="alert alert-warning" style="margin: 12px 16px">
            <AppIcon name="alert" :size="16" class="icon" />
            <span>{{ bansError }}</span>
          </div>

          <div v-if="!bansError && filteredBans.length" class="table-wrap">
            <table class="table">
              <thead>
                <tr>
                  <th>状态</th>
                  <th>域名</th>
                  <th>客户端 IP</th>
                  <th>原因</th>
                  <th>来源节点</th>
                  <th>创建时间</th>
                  <th>到期 / 解封</th>
                  <th class="text-right">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="b in pagedBans" :key="`${b.domain}|${b.ip}`">
                  <td>
                    <StatusTag :tone="banTone(b.status)" dot>{{ banStatusLabel(b.status) }}</StatusTag>
                  </td>
                  <td class="cell-main">{{ b.domain }}</td>
                  <td class="mono">{{ b.ip }}</td>
                  <td class="text-2">{{ b.reason || 'cc-auto-ban' }}</td>
                  <td class="text-2">{{ b.source || '—' }}</td>
                  <td>
                    <div>{{ formatDateTime(b.created_at) }}</div>
                  </td>
                  <td>
                    <template v-if="b.status === 'active'">
                      <div>{{ formatDateTime(b.expires_at) }}</div>
                      <div class="cell-sub">剩余 {{ remainingLabel(b.remaining_secs) }}</div>
                    </template>
                    <template v-else-if="b.status === 'revoked'">
                      <div>{{ formatDateTime(b.revoked_at) }}</div>
                      <div class="cell-sub">由 {{ b.revoked_by || '管理员' }} 解封</div>
                    </template>
                    <template v-else>
                      <div>{{ formatDateTime(b.expires_at) }}</div>
                      <div class="cell-sub">已过期</div>
                    </template>
                  </td>
                  <td class="text-right">
                    <button v-if="b.status === 'active'" class="btn btn-text btn-sm danger" type="button"
                      @click="unbanIp(b)">
                      解封
                    </button>
                    <span v-else class="text-muted" style="font-size: 12px">—</span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <div v-else-if="bansLoading" class="card-body">
            <div class="skeleton" style="height: 32px; width: 100%" />
          </div>
          <EmptyState v-else-if="!bansError" icon="shield" :title="bans.length ? '没有匹配的封禁记录' : '暂无共享封禁'" :text="bans.length
              ? '调整状态筛选条件后再试。'
              : '当前防护域名暂无封禁记录；已过期和已解封记录也会保留。'
            " />
          <div v-if="!bansError && filteredBans.length > banPageSize" class="table-footer">
            <AppPagination
              v-model:page="banPage"
              v-model:page-size="banPageSize"
              :total="filteredBans.length"
              :page-size="banPageSize"
              :page-sizes="[20, 50, 100]"
            />
          </div>
        
  </div>
</template>
