<script setup lang="ts">
import { displayVersion } from '~/utils/version'
const settings = inject('settingsConsole') as ReturnType<typeof useSettingsConsole>
const {
  saveBrandSettings,
  checkUpdate,
  applyUpdate,
  auth,
  brandNameDraft,
  brandSaving,
  controlVersion,
  updateInfo,
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
          <div class="card-title">品牌设置</div>
          <div class="card-desc">自定义管理控制台显示名称</div>
        </div>
      </div>
      <div class="card-body">
        <div class="field" style="max-width: 520px">
          <label class="field-label">系统名称</label>
          <input v-model="brandNameDraft" class="input" maxlength="64" placeholder="例如 我的 CDN" />
          <span class="field-hint">建议 2–20 字；用于侧栏、页脚与登录页。</span>
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
          <div class="card-desc">检查可用版本并升级当前系统组件</div>
        </div>
        <StatusTag
          v-if="updateInfo && !updateInfo.error"
          :tone="updateInfo.update_available ? 'warning' : 'success'"
          dot
        >
          {{ updateInfo.update_available ? '有新版本' : '已是最新' }}
        </StatusTag>
      </div>
      <div class="card-body">
        <div class="update-grid">
          <div class="update-metric">
            <div class="info-label">当前版本</div>
            <div class="info-value mono">
              {{ displayVersion(updateInfo?.current_version || controlVersion) || '—' }}
            </div>
          </div>
          <div class="update-metric">
            <div class="info-label">最新版本</div>
            <div class="info-value mono">{{ displayVersion(updateInfo?.latest_version) || '—' }}</div>
          </div>
          <div class="update-metric">
            <div class="info-label">更新组件</div>
            <div class="info-value">控制面 / Edge / DNS / Web</div>
          </div>
        </div>

        <div v-if="updateInfo?.update_available" class="update-banner">
          <AppIcon name="upload" :size="16" />
          <span>
            检测到新版本 <b>{{ displayVersion(updateInfo.latest_version) }}</b>，更新过程中控制面会短暂重启。
          </span>
        </div>
        <div v-else-if="updateInfo?.error" class="field-hint text-danger mt-12">
          {{ updateInfo.error }}
        </div>

        <div class="flex gap-8 mt-16" style="flex-wrap: wrap">
          <button
            class="btn btn-default btn-sm"
            type="button"
            :disabled="updateChecking"
            @click="checkUpdate"
          >
            <AppIcon name="refresh" :size="15" />
            {{ updateChecking ? '检测中…' : '检测更新' }}
          </button>
          <button
            v-if="updateInfo?.update_available"
            class="btn btn-primary btn-sm"
            type="button"
            :disabled="updateBusy || !updateInfo.updater_available"
            @click="applyUpdate"
          >
            <AppIcon name="upload" :size="15" />
            {{ updateBusy ? '更新中…' : '立即更新' }}
          </button>
        </div>
        <p v-if="updateInfo && !updateInfo.updater_available" class="field-hint text-warning mt-12">
          当前安装未配置 root 更新助手，请重新执行 <code>install.sh control</code>。
        </p>
        <p v-if="updateMessage" class="field-hint mt-12">{{ updateMessage }}</p>
        <p v-if="updateStatusInfo?.message" class="field-hint mt-8">
          更新状态：{{ updateStatusInfo.state }} · {{ updateStatusInfo.message }}
        </p>
      </div>
    </div>
  </div>
</template>
