<script setup lang="ts">
const settings = inject('settingsConsole') as ReturnType<typeof useSettingsConsole>
const {
  saveBrandSettings,
  checkUpdate,
  applyUpdate,
  auth,
  brandNameDraft,
  updateRepoDraft,
  hostRepo,
  effectiveRepo,
  repoPassword,
  updateTokenDraft,
  updateTokenConfigured,
  clearUpdateToken,
  brandSaving,
  updateRepoConfigured,
  credentialsChanged,
  controlVersion,
  updateInfo,
  releaseLink,
  updateChecking,
  updateBusy,
  updateMessage,
  updateStatusInfo,
  geoipInfo,
  geoipLoading,
  geoipUpdating,
  loadGeoip,
  updateGeoip,
  formatBytes,
} = settings

watchEffect(() => {
  if (!auth.isAdmin) navigateTo('/settings', { replace: true })
})
</script>

<template>
  <div class="settings-advanced">
    <div v-if="auth.isAdmin" class="card mt-16">
      <div class="card-head">
        <div>
          <div class="card-title">品牌与升级源</div>
          <div class="card-desc">系统名称、升级仓库与访问令牌；未配置时系统升级不可用，安装脚本仍需仓库地址</div>
        </div>
      </div>
      <div class="card-body">
        <div class="field" style="max-width: 520px">
          <label class="field-label">系统名称</label>
          <input v-model="brandNameDraft" class="input" maxlength="64" placeholder="例如 我的 CDN" />
          <span class="field-hint">建议 2–20 字；用于侧栏、页脚与登录页。</span>
        </div>
        <div class="field mt-16" style="max-width: 520px">
          <label class="field-label">升级仓库</label>
          <input
            v-model="updateRepoDraft"
            class="input mono"
            maxlength="200"
            placeholder="owner/repo"
            spellcheck="false"
          />
          <span class="field-hint">
            格式 <span class="mono">owner/repo</span>，用于检测更新、远程安装与添加节点向导的安装命令。
            当前生效：<span class="mono">{{ effectiveRepo || '未配置' }}</span>。
          </span>
        </div>
        <div class="field mt-16" style="max-width: 520px">
          <label class="field-label">访问令牌</label>
          <input
            v-model="updateTokenDraft"
            class="input mono"
            type="password"
            maxlength="200"
            autocomplete="new-password"
            :disabled="clearUpdateToken"
            :placeholder="updateTokenConfigured && !clearUpdateToken ? '已配置，留空表示不修改' : '私有仓 / Release 读取令牌'"
            spellcheck="false"
          />
          <div class="flex gap-8 mt-8" style="align-items: center; flex-wrap: wrap">
            <StatusTag :tone="updateTokenConfigured && !clearUpdateToken ? 'success' : 'warning'" dot>
              {{ updateTokenConfigured && !clearUpdateToken ? '已配置' : '未配置' }}
            </StatusTag>
            <label v-if="updateTokenConfigured" class="check" style="margin: 0">
              <input v-model="clearUpdateToken" type="checkbox" />
              清除已保存的令牌
            </label>
          </div>
          <span class="field-hint">
            与升级仓库一并保存后，才可使用「系统升级」。也可由 control.env 的
            <span class="mono">LAGRAN_GITHUB_TOKEN</span> 提供。
          </span>
        </div>
        <div v-if="credentialsChanged" class="field mt-12" style="max-width: 520px">
          <label class="field-label">当前登录密码</label>
          <input
            v-model="repoPassword"
            class="input"
            type="password"
            autocomplete="current-password"
            placeholder="修改升级仓库或令牌需要验证密码"
          />
          <span class="field-hint">该仓库的安装脚本会以 root 身份在远程安装的节点上运行。</span>
        </div>
        <div class="flex gap-8 mt-16">
          <button
            class="btn btn-primary"
            type="button"
            :disabled="brandSaving || !brandNameDraft.trim()"
            @click="saveBrandSettings"
          >
            {{ brandSaving ? '保存中…' : '保存' }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="auth.isAdmin" class="card mt-16">
      <div class="card-head">
        <div>
          <div class="card-title">IP 地理位置库</div>
          <div class="card-desc">
            MaxMind GeoLite2-Country；控制面热加载后推送给边缘节点，不中断业务流量
          </div>
        </div>
        <StatusTag :tone="geoipInfo?.loaded ? 'success' : 'warning'" dot>
          {{ geoipInfo?.loaded ? '已加载' : '未加载' }}
        </StatusTag>
      </div>
      <div class="card-body">
        <div class="update-grid">
          <div class="update-metric">
            <div class="info-label">大小</div>
            <div class="info-value mono">{{ formatBytes(geoipInfo?.size_bytes) }}</div>
          </div>
          <div class="update-metric">
            <div class="info-label">来源</div>
            <div class="info-value">{{ geoipInfo?.source || '—' }}</div>
          </div>
          <div class="update-metric">
            <div class="info-label">更新时间</div>
            <div class="info-value mono" style="font-size: 12px">{{ geoipInfo?.updated_at || '—' }}</div>
          </div>
        </div>
        <div class="field-hint mt-12">
          SHA256：<span class="mono">{{ geoipInfo?.sha256 || '—' }}</span>
        </div>
        <div class="field-hint mt-8">
          更新地址：社区镜像
          <span class="mono">wp-statistics/GeoLite2-Country</span>
          （免注册）。边缘节点通过心跳/通知拉取同一份库。
        </div>
        <div class="flex gap-8 mt-16" style="flex-wrap: wrap">
          <button
            class="btn btn-default btn-sm"
            type="button"
            :disabled="geoipLoading || geoipUpdating"
            @click="loadGeoip"
          >
            <AppIcon name="refresh" :size="15" />
            {{ geoipLoading ? '刷新中…' : '刷新状态' }}
          </button>
          <button
            class="btn btn-primary btn-sm"
            type="button"
            :disabled="geoipUpdating"
            @click="updateGeoip"
          >
            <AppIcon name="upload" :size="15" />
            {{ geoipUpdating ? '更新中…' : '更新 IP 库' }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="auth.isAdmin" class="card mt-16">
      <div class="card-head">
        <div>
          <div class="card-title">系统升级</div>
          <div class="card-desc">从已配置的升级仓库检测并安装新版本</div>
        </div>
        <StatusTag
          v-if="updateRepoConfigured && updateInfo && !updateInfo.error"
          :tone="updateInfo.update_available ? 'warning' : 'success'"
          dot
        >
          {{ updateInfo.update_available ? '有新版本' : '已是最新' }}
        </StatusTag>
      </div>
      <div class="card-body" :class="{ 'is-disabled': !updateRepoConfigured }">
        <div class="update-grid">
          <div class="update-metric">
            <div class="info-label">当前版本</div>
            <div class="info-value mono">{{ updateInfo?.current_version || controlVersion || '—' }}</div>
          </div>
          <div class="update-metric">
            <div class="info-label">最新版本</div>
            <div class="info-value mono">{{ updateRepoConfigured ? (updateInfo?.latest_version || '—') : '—' }}</div>
          </div>
          <div class="update-metric">
            <div class="info-label">更新组件</div>
            <div class="info-value">控制面 / Edge / DNS / Web</div>
          </div>
        </div>

        <div v-if="!updateRepoConfigured" class="field-hint text-warning mt-12">
          请先在上方填写并保存「升级仓库」与「访问令牌」，保存后才能检测与安装更新。安装脚本仍需要仓库地址。
        </div>
        <template v-else>
          <div v-if="hostRepo" class="field-hint mt-12">
            本机升级助手仓库：<span class="mono">{{ hostRepo }}</span>（由 install.sh 写入 update.env）。
          </div>
          <div v-else class="field-hint text-warning mt-12">
            本机升级助手尚未写入仓库；「立即更新」可能失败，请重新执行 install.sh control。
          </div>
          <div v-if="updateInfo?.update_available" class="update-banner">
            <AppIcon name="upload" :size="16" />
            <span>
              检测到新版本 <b>{{ updateInfo.latest_version }}</b>，更新过程中控制面会短暂重启。
            </span>
          </div>
          <div v-else-if="updateInfo?.error" class="field-hint text-danger mt-12">
            {{ updateInfo.error }}
          </div>
        </template>

        <div class="flex gap-8 mt-16" style="flex-wrap: wrap">
          <button
            class="btn btn-default btn-sm"
            type="button"
            :disabled="!updateRepoConfigured || updateChecking"
            @click="checkUpdate"
          >
            <AppIcon name="refresh" :size="15" />
            {{ updateChecking ? '检测中…' : '检测更新' }}
          </button>
          <button
            v-if="updateRepoConfigured && updateInfo?.update_available"
            class="btn btn-primary btn-sm"
            type="button"
            :disabled="updateBusy || !updateInfo.updater_available"
            @click="applyUpdate"
          >
            <AppIcon name="upload" :size="15" />
            {{ updateBusy ? '更新中…' : '立即更新' }}
          </button>
          <a
            v-if="updateRepoConfigured && releaseLink"
            class="btn btn-default btn-sm"
            :href="releaseLink"
            target="_blank"
            rel="noopener"
          >
            查看发布说明
          </a>
        </div>
        <p v-if="updateRepoConfigured && updateInfo && !updateInfo.updater_available" class="field-hint text-warning mt-12">
          当前安装未配置 root 更新助手，请重新执行 <code>install.sh control</code>。
        </p>
        <p v-if="updateRepoConfigured && updateMessage" class="field-hint mt-12">{{ updateMessage }}</p>
        <p v-if="updateRepoConfigured && updateStatusInfo?.message" class="field-hint mt-8">
          更新状态：{{ updateStatusInfo.state }} · {{ updateStatusInfo.message }}
        </p>
      </div>
    </div>
  </div>
</template>
