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

          <div class="alert" :class="settings.mode === 'block' ? 'alert-warning' : 'alert-info'">
            <AppIcon name="shield" :size="16" class="icon" />
            <span v-if="settings.mode === 'block'">
              「拦截」会直接拒绝命中规则的请求（返回 403）。上线初期建议先用「仅记录」确认没有误拦。
            </span>
            <span v-else>
              「仅记录」只记事件、不拦截请求。确认规则准确后，再切换为拦截。
            </span>
          </div>

          <div class="setting-row">
            <div>
              <div class="setting-title">开启 WAF 防护</div>
              <div class="setting-desc">关闭后该域名的 IP 名单与规则全部不生效</div>
            </div>
            <label class="switch" :class="{ on: settings.enabled }">
              <span class="switch-track" />
              <input v-model="settings.enabled" type="checkbox" style="display: none" />
            </label>
          </div>

          <div class="setting-row">
            <div>
              <div class="setting-title">防护模式</div>
              <div class="setting-desc">仅记录用于试跑规则，拦截用于正式防护</div>
            </div>
            <div class="segmented">
              <button type="button" :class="{ active: settings.mode === 'monitor' }" @click="settings.mode = 'monitor'">
                仅记录
              </button>
              <button type="button" :class="{ active: settings.mode === 'block' }" @click="settings.mode = 'block'">
                拦截
              </button>
            </div>
          </div>

          <div class="section-label">浏览器质询</div>
          <div class="segmented level-seg">
            <button v-for="l in securityLevels" :key="l.value" type="button"
              :class="{ active: settings.security_level === l.value }" @click="settings.security_level = l.value">
              {{ l.label }}
            </button>
          </div>
          <p class="text-muted" style="font-size: 12px; margin-top: 8px">
            {{ levelHint }} WAF 总开关关闭时，所有 WAF 检查都不生效。
          </p>

          <div class="section-label">托管规则集</div>
          <div class="ruleset-grid">
            <label v-for="rs in rulesets" :key="rs.id" class="ruleset-card"
              :class="{ on: settings.managed_rulesets.includes(rs.id) }">
              <input type="checkbox" :checked="settings.managed_rulesets.includes(rs.id)"
                @change="toggleRuleset(rs.id)" />
              <span class="ruleset-body">
                <span class="ruleset-head">
                  <strong>{{ rs.name }}</strong>
                  <span class="ruleset-version">{{ rs.version }}</span>
                  <span v-if="rs.recommended" class="ruleset-badge">推荐</span>
                </span>
                <small>{{ rs.description }}</small>
                <span class="ruleset-categories">
                  <span v-for="(label, i) in rs.category_labels" :key="rs.categories[i]" class="ruleset-chip">
                    {{ label }}
                  </span>
                </span>
              </span>
              <AppIcon :name="settings.managed_rulesets.includes(rs.id) ? 'check' : 'plus'" :size="15" />
            </label>
            <div v-if="!rulesets.length" class="text-muted" style="font-size: 13px">
              正在加载托管规则集…
            </div>
          </div>
          <p class="text-muted" style="font-size: 12px; margin-top: 8px">
            规则集是增量启用：选中后会启用其覆盖的签名类别，不会关闭下方单独开启的规则。规则集版本由控制面统一维护。
          </p>

          <div class="section-label">规则包版本与灰度</div>
          <div class="form-grid">
            <div class="field">
              <label class="field-label">规则包版本</label>
              <select v-model="settings.waf_rulepack_version" class="select">
                <option value="">跟随 active 版本</option>
                <option v-for="p in rulepackVersions" :key="p.version" :value="p.version">
                  {{ p.version }}（{{ p.slot }}）
                </option>
              </select>
              <span class="field-hint">留空跟随控制面 active；固定版本可用于单域名验证/回滚。</span>
            </div>
            <div class="field">
              <label class="field-label">灰度比例</label>
              <input v-model.number="settings.waf_gray_percent" class="input" type="number" min="0" max="100" />
              <span class="field-hint">0 = 全量 active；1~99 按域名稳定哈希切分到 active/previous。</span>
            </div>
          </div>

          <div v-if="auth.isAdmin" class="rulepack-admin">
            <div class="flex-between">
              <span class="text-muted" style="font-size: 12px">
                规则包由 Control 签名，Edge 验签后缓存到本地。回滚只切换 active/previous。
                发布前应先在 Control 中更新内置 catalog；catalog 无变化时发布会返回 409。
              </span>
              <button class="btn btn-default btn-sm" type="button" :disabled="rulepackBusy" @click="publishRulepack">
                发布新版本
              </button>
            </div>
            <div v-if="rulepackVersions.length" class="rulepack-versions">
              <div v-for="p in rulepackVersions" :key="p.version" class="rulepack-version-row"
                :class="`slot-${p.slot}`">
                <div class="version-main">
                  <div class="version-title">
                    <span class="mono version-name">{{ p.version }}</span>
                    <span class="slot-badge" :class="`slot-badge-${p.slot}`">
                      <span class="slot-dot" />
                      {{ slotLabel(p.slot) }}
                    </span>
                  </div>
                  <div class="version-meta">
                    <span>创建 {{ formatRulepackTime(p.created_at) }}</span>
                    <span class="meta-sep">·</span>
                    <span class="mono" :title="p.signature">签名 {{ p.signature.slice(0, 10) }}</span>
                  </div>
                </div>

                <div class="version-actions">
                  <span v-if="p.slot === 'active'" class="current-label">
                    <AppIcon name="check" :size="13" />
                    当前生效
                  </span>
                  <template v-else>
                    <button class="btn btn-default btn-sm version-action" type="button" :disabled="rulepackBusy"
                      @click="rollbackRulepack(p.version)">
                      设为 active
                    </button>
                    <button v-if="p.slot === 'previous'" class="btn btn-text btn-sm version-action" type="button"
                      :disabled="rulepackBusy" @click="retireRulepack(p.version)">
                      转为 retired
                    </button>
                    <button v-if="p.slot === 'retired'" class="btn btn-text btn-sm danger version-action" type="button"
                      :disabled="rulepackBusy" @click="deleteRulepack(p.version)">
                      删除
                    </button>
                  </template>
                </div>
              </div>
            </div>
          </div>

          <div class="section-label">单条托管规则</div>
          <div class="managed-grid">
            <label v-for="m in managedRules" :key="m.key" class="managed-item" :class="{ on: settings[m.key] }">
              <input v-model="settings[m.key]" type="checkbox" />
              <span class="managed-info">
                <strong>{{ m.label }}</strong>
                <small>{{ m.desc }}</small>
              </span>
              <AppIcon :name="settings[m.key] ? 'check' : 'plus'" :size="15" />
            </label>
          </div>

          <div class="section-label">浏览器安全验证</div>
          <div class="form-grid">
            <div class="field">
              <label class="field-label">验证难度</label>
              <input v-model.number="settings.challenge_difficulty" class="input" type="number" min="1" max="6" />
              <span class="field-hint">1 最轻、6 最重，默认 4。数值越高，访客通过验证等待越久。</span>
            </div>
            <div class="field">
              <label class="field-label">免验证有效期（秒）</label>
              <input v-model.number="settings.challenge_ttl_secs" class="input" type="number" min="60" max="86400" />
              <span class="field-hint">通过后一段时间内不再验证，默认 1800 秒（30 分钟）。</span>
            </div>
          </div>
          <p class="text-muted" style="font-size: 12px; margin-top: 8px">
            规则动作选择「安全验证」时生效；一般只对网页浏览弹出验证页。
            接口请求、表单提交等通常会跳过验证直接放行；如需限制请改用拦截规则或限流。
          </p>

          <div class="flex-between mt-24">
            <span class="text-muted" style="font-size: 12px">
              托管规则基于常见攻击特征匹配 URL、查询串、UA 与 Referer，不检查请求体。
            </span>
            <button class="btn btn-primary" type="button" :disabled="saving" @click="saveSettings">
              {{ saving ? '保存中…' : '保存设置' }}
            </button>
          </div>
        
  </div>
</template>
