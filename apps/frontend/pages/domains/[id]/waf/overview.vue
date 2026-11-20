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
  <div class="card-body">

          <div class="stat-grid">
            <StatCard label="事件总数" :value="stats.total" icon="shield" />
            <StatCard label="拦截" :value="actionCount('block')" tone="red" icon="alert" />
            <StatCard label="安全验证" :value="actionCount('challenge')" tone="orange" icon="zap" />
            <StatCard label="观察 / 记录" :value="actionCount('monitor') + actionCount('log')" tone="green"
              icon="activity" />
          </div>

          <div class="section-label">按天趋势（最近 {{ stats.days }} 天）</div>
          <div v-if="stats.by_day.length" class="bar-chart">
            <div v-for="d in stats.by_day" :key="d.key" class="bar-col" :title="`${d.key}: ${d.count}`">
              <div class="bar" :style="{ height: barHeight(d.count) }" />
              <span class="bar-label">{{ d.key.slice(5) }}</span>
            </div>
          </div>
          <div v-else class="text-muted" style="font-size: 13px; padding: 12px 0">
            最近 {{ stats.days }} 天暂无 WAF 事件。
          </div>

          <div class="waf-stat-grid">
            <div class="stat-list">
              <div class="stat-list-title">按类型</div>
              <div v-for="c in stats.by_category" :key="c.key" class="stat-list-row">
                <span>{{ categoryLabel(c.key) }}</span>
                <b>{{ c.count }}</b>
              </div>
              <div v-if="!stats.by_category.length" class="stat-empty">暂无</div>
            </div>
            <div class="stat-list">
              <div class="stat-list-title">Top 客户端 IP</div>
              <div v-for="c in stats.top_ips" :key="c.key" class="stat-list-row">
                <span class="mono">{{ c.key }}</span>
                <b>{{ c.count }}</b>
              </div>
              <div v-if="!stats.top_ips.length" class="stat-empty">暂无</div>
            </div>
            <div class="stat-list">
              <div class="stat-list-title">Top 命中规则</div>
              <div v-for="c in stats.top_rules" :key="c.key" class="stat-list-row">
                <span>{{ c.key }}</span>
                <b>{{ c.count }}</b>
              </div>
              <div v-if="!stats.top_rules.length" class="stat-empty">暂无</div>
            </div>
            <div class="stat-list">
              <div class="stat-list-title">Top 路径</div>
              <div v-for="c in stats.top_paths" :key="c.key" class="stat-list-row">
                <span class="mono truncate" :title="c.key">{{ c.key }}</span>
                <b>{{ c.count }}</b>
              </div>
              <div v-if="!stats.top_paths.length" class="stat-empty">暂无</div>
            </div>
          </div>
        
  </div>
</template>
