<script setup lang="ts">
import type { useNodeDetail } from '~/composables/useNodeDetail'

const ctx = inject('nodeDetail') as ReturnType<typeof useNodeDetail>
const {
  detail,
  detailProfile,
  detailProfileStored,
  detailPlan,
  detailHardwareSummary,
  detailHasOverrides,
  detailListenerWarning,
  detailCallbackWarning,
  detailEnv,
  savingProfile,
  CACHE_BACKEND_LABEL,
  humanBytes,
  setDetailDiskType,
  setDetailWorkload,
  setDetailRole,
  setDetailCacheBackend,
  saveDetailProfile,
  clearDetailOverrides,
  copy,
  downloadDetailEnv,
} = ctx
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">硬件画像</div>
        <div class="card-desc">调整后重算 edge.env，并同步 AGENT_URL</div>
      </div>
      <div class="card-extra">
        <button
          v-if="detailHasOverrides"
          class="btn btn-default btn-sm"
          type="button"
          :disabled="savingProfile"
          @click="clearDetailOverrides"
        >
          清除手工参数
        </button>
        <button class="btn btn-primary btn-sm" type="button" :disabled="savingProfile" @click="saveDetailProfile">
          {{ savingProfile ? '保存中…' : '保存画像' }}
        </button>
      </div>
    </div>
    <div class="card-body form-col">
      <div v-if="!detailProfileStored" class="alert alert-info">
        <AppIcon name="info" :size="16" class="icon" />
        <span>该节点尚未保存硬件画像，当前为默认值；保存后详情页会按此配置复现 edge.env。</span>
      </div>
      <div v-if="detailListenerWarning || detailCallbackWarning" class="alert alert-warning">
        <AppIcon name="alert" :size="16" class="icon" />
        <span>{{ detailListenerWarning || detailCallbackWarning }}</span>
      </div>

      <div class="profile-grid">
        <label class="profile-field">
          <span>CPU 核数</span>
          <input v-model.number="detailProfile.hardware.cpuCores" class="input" type="number" min="1" max="128" />
        </label>
        <label class="profile-field">
          <span>内存（GiB）</span>
          <input v-model.number="detailProfile.hardware.memoryGb" class="input" type="number" min="1" max="512" />
        </label>
        <label class="profile-field">
          <span>缓存磁盘（GiB）</span>
          <input v-model.number="detailProfile.hardware.diskGb" class="input" type="number" min="0" max="100000" />
        </label>
        <label class="profile-field">
          <span>磁盘类型</span>
          <select
            class="select"
            :value="detailProfile.hardware.diskType"
            @change="setDetailDiskType(($event.target as HTMLSelectElement).value)"
          >
            <option value="nvme">NVMe SSD</option>
            <option value="sata-ssd">SATA SSD</option>
            <option value="hdd">机械盘 HDD</option>
          </select>
        </label>
        <label class="profile-field">
          <span>出口带宽（Mbps）</span>
          <input v-model.number="detailProfile.hardware.bandwidthMbps" class="input" type="number" min="0" max="100000" />
        </label>
        <label class="profile-field">
          <span>业务类型</span>
          <select
            class="select"
            :value="detailProfile.hardware.workload"
            @change="setDetailWorkload(($event.target as HTMLSelectElement).value)"
          >
            <option value="web">网页 / 小文件</option>
            <option value="mixed">混合业务</option>
            <option value="video">视频 / 大文件点播</option>
          </select>
        </label>
        <label class="profile-field">
          <span>节点角色</span>
          <select
            class="select"
            :value="detailProfile.hardware.role"
            @change="setDetailRole(($event.target as HTMLSelectElement).value)"
          >
            <option value="edge">普通边缘节点</option>
            <option value="shield">L2 节点（汇聚层）</option>
          </select>
        </label>
        <label class="profile-field span-2 checkbox">
          <input v-model="detailProfile.hardware.sharedRedis" type="checkbox" />
          <span>使用共享 Redis（区域实例即可）</span>
        </label>
        <label class="profile-field span-2 checkbox">
          <input v-model="detailProfile.infra.httpsEnabled" type="checkbox" />
          <span>启用 HTTPS 监听</span>
        </label>
        <label class="profile-field">
          <span>HTTP 监听（EDGE_LISTEN）</span>
          <input v-model="detailProfile.infra.httpListen" class="input mono" placeholder="0.0.0.0:80" />
        </label>
        <label class="profile-field">
          <span>HTTPS 监听</span>
          <input
            v-model="detailProfile.infra.httpsListen"
            class="input mono"
            :disabled="!detailProfile.infra.httpsEnabled"
            placeholder="0.0.0.0:443"
          />
        </label>
        <label class="profile-field">
          <span>管理面监听（EDGE_ADMIN_LISTEN）</span>
          <input v-model="detailProfile.infra.adminListen" class="input mono" placeholder="0.0.0.0:8081" />
        </label>
        <label class="profile-field span-2">
          <span>AGENT_URL（控制面回调）</span>
          <input
            v-model="detailProfile.infra.agentUrl"
            class="input mono"
            :placeholder="detail?.node?.agent_url || 'http://<控制面可达地址>:8081'"
          />
          <em class="profile-note">
            必须指向管理面端口且控制面可达；与 EDGE_ADMIN_LISTEN 端口不一致会让 purge / 证书 / ACME 回调失败。
          </em>
        </label>
        <label class="profile-field">
          <span>CACHE_BACKEND</span>
          <select
            class="select"
            :value="detailProfile.infra.cacheBackend"
            @change="setDetailCacheBackend(($event.target as HTMLSelectElement).value)"
          >
            <option v-for="(label, value) in CACHE_BACKEND_LABEL" :key="value" :value="value">
              {{ label }}
            </option>
          </select>
        </label>
        <label class="profile-field">
          <span>REDIS_URL</span>
          <input v-model="detailProfile.infra.redisUrl" class="input mono" placeholder="redis://127.0.0.1:6379/0" />
        </label>
        <label class="profile-field span-2">
          <span>CACHE_REDIS_URL（留空复用 REDIS_URL）</span>
          <input v-model="detailProfile.infra.cacheRedisUrl" class="input mono" placeholder="redis://127.0.0.1:6379/1" />
        </label>
        <label class="profile-field">
          <span>CHALLENGE_SECRET</span>
          <input
            v-model="detailProfile.infra.challengeSecret"
            class="input mono"
            type="password"
            autocomplete="off"
            placeholder="各节点必须一致"
          />
        </label>
        <label class="profile-field">
          <span>WAF_RULEPACK_SECRET</span>
          <input
            v-model="detailProfile.infra.wafRulepackSecret"
            class="input mono"
            type="password"
            autocomplete="off"
            placeholder="与控制面一致"
          />
        </label>
      </div>

      <div class="flex-between profile-actions">
        <span class="text-muted" style="font-size: 12px">
          {{ detailHardwareSummary }} · Sizing:
          {{ humanBytes(detailPlan.metrics.memoryCacheBytes) }} 内存缓存
          <template v-if="detailPlan.metrics.largeCacheEnabled">
            · {{ humanBytes(detailPlan.metrics.largeCacheBytes) }} 大对象缓存
          </template>
          <template v-if="detailHasOverrides"> · 含手工参数</template>
        </span>
        <span class="profile-actions-right">
          <button class="btn btn-text btn-sm" type="button" @click="copy(detailEnv, 'edge.env')">复制 edge.env</button>
          <button class="btn btn-text btn-sm" type="button" @click="downloadDetailEnv">下载</button>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.form-col {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.profile-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 10px;
}

.profile-field {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  font-size: 11.5px;
  color: var(--text-3);
}

.profile-field .input,
.profile-field .select {
  height: 32px;
}

.profile-field.span-2 {
  grid-column: span 2;
}

.profile-field.checkbox {
  flex-direction: row;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  color: var(--text-2);
}

.profile-note {
  font-style: normal;
  font-size: 11px;
  color: var(--text-3);
  line-height: 1.4;
}

.profile-actions {
  margin-top: 4px;
}

.profile-actions-right {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

@media (max-width: 640px) {
  .profile-grid {
    grid-template-columns: 1fr;
  }

  .profile-field.span-2 {
    grid-column: span 1;
  }
}
</style>
