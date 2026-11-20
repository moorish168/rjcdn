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
              白名单优先于所有规则；黑名单命中后直接拦截。支持单条 IP/CIDR，也可引用可复用 IP 名单
            </span>
            <span class="spacer" />
            <button class="btn btn-default btn-sm" type="button" @click="openBindListModal">
              引用 IP 名单
            </button>
            <button class="btn btn-primary btn-sm" type="button" @click="openIpModal('deny')">
              <AppIcon name="plus" :size="14" />
              添加 IP / 网段
            </button>
          </div>

          <div v-if="loading" class="text-muted" style="padding: 24px 0">加载 IP 名单中…</div>
          <template v-else>
            <div v-if="ipBindings.length" class="table-wrap" style="margin-bottom: 16px">
              <div class="section-label" style="margin: 8px 0; font-size: 13px; color: var(--text-muted)">
                已引用的可复用名单
                <NuxtLink to="/ip-lists" class="btn btn-text btn-sm" style="margin-left: 8px">管理名单库</NuxtLink>
              </div>
              <table class="table">
                <thead>
                  <tr>
                    <th>名单</th>
                    <th>类型</th>
                    <th>状态</th>
                    <th class="text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="b in ipBindings" :key="b.id">
                    <td>{{ b.list_name }}</td>
                    <td>
                      <StatusTag :tone="b.list_type === 'white' ? 'success' : 'danger'">
                        {{ b.list_type === 'white' ? '白名单' : '黑名单' }}
                      </StatusTag>
                    </td>
                    <td>
                      <StatusTag :tone="b.enabled ? 'success' : 'neutral'" dot>
                        {{ b.enabled ? '启用' : '停用' }}
                      </StatusTag>
                    </td>
                    <td class="text-right">
                      <button class="btn btn-text btn-sm danger" type="button" @click="unbindIpList(b)">
                        解除引用
                      </button>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>

            <div v-if="ips.length" class="table-wrap">
              <table class="table">
                <thead>
                  <tr>
                    <th>类型</th>
                    <th>IP / 网段</th>
                    <th>备注</th>
                    <th>添加时间</th>
                    <th>状态</th>
                    <th class="text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="ip in pagedIps" :key="ip.id">
                    <td>
                      <StatusTag :tone="ip.kind === 'allow' ? 'success' : 'danger'">
                        {{ ip.kind === 'allow' ? '白名单' : '黑名单' }}
                      </StatusTag>
                    </td>
                    <td class="mono">{{ ip.ip }}</td>
                    <td class="text-2">{{ ip.note || '—' }}</td>
                    <td class="text-muted">{{ formatDateTime(ip.created_at) }}</td>
                    <td>
                      <StatusTag :tone="ip.enabled ? 'success' : 'neutral'" dot>
                        {{ ip.enabled ? '启用' : '停用' }}
                      </StatusTag>
                    </td>
                    <td class="text-right">
                      <div class="ops" style="justify-content: flex-end">
                        <button class="btn btn-text btn-sm" type="button" @click="toggleIp(ip)">
                          {{ ip.enabled ? '停用' : '启用' }}
                        </button>
                        <button class="btn btn-text btn-sm" type="button" @click="openIpModal(ip.kind, ip)">
                          编辑
                        </button>
                        <button class="btn btn-text btn-sm danger" type="button" @click="removeIp(ip)">
                          删除
                        </button>
                      </div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div v-if="ips.length > ipPageSize" class="table-footer">
              <AppPagination v-model:page="ipPage" :total="ips.length" :page-size="ipPageSize" />
            </div>
            <EmptyState
              v-if="!ips.length && !ipBindings.length"
              icon="shield"
              title="还没有 IP 名单"
              text="添加单条 IP/CIDR，或从「IP 名单」库引用可复用黑白名单。"
            >
              <button class="btn btn-default btn-sm" type="button" @click="openBindListModal">引用名单</button>
              <button class="btn btn-primary btn-sm" type="button" @click="openIpModal('deny')">
                添加第一条
              </button>
            </EmptyState>
          </template>
        
  </div>
</template>
