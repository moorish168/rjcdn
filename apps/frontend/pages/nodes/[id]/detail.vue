<script setup lang="ts">
import type { useNodeDetail } from '~/composables/useNodeDetail'

const ctx = inject('nodeDetail') as ReturnType<typeof useNodeDetail>
const {
  detail,
  detailEnv,
  detailHardwareSummary,
  detailProfileStored,
  detailListenerWarning,
  detailCallbackWarning,
  detailPlan,
  copy,
  downloadDetailEnv,
  statusLabel,
  statusTone,
} = ctx
</script>

<template>
  <div v-if="detail" class="card">
    <div class="card-head">
      <div class="card-title">节点详情</div>
    </div>
    <div class="card-body form-col">
      <div class="detail-grid">
        <div class="detail-item">
          <div class="detail-label">节点 ID</div>
          <div class="detail-value mono">{{ detail.node.id }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">状态</div>
          <div class="detail-value">
            <StatusTag :tone="statusTone(detail.node.status)" dot>
              {{ statusLabel(detail.node.status) }}
            </StatusTag>
          </div>
        </div>
        <div class="detail-item">
          <div class="detail-label">区域</div>
          <div class="detail-value">{{ detail.node.region || '默认区域' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">版本</div>
          <div class="detail-value">{{ detail.node.version || '—' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">公网 IP</div>
          <div class="detail-value mono">{{ detail.node.public_ip || '未配置' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">国家/地区</div>
          <div class="detail-value">{{ detail.node.country || 'CN' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">省份 / 城市</div>
          <div class="detail-value">
            {{ [detail.node.province, detail.node.city].filter(Boolean).join(' / ') || '未配置' }}
          </div>
        </div>
        <div class="detail-item">
          <div class="detail-label">运营商</div>
          <div class="detail-value">{{ detail.node.isp || '未配置' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">调度权重</div>
          <div class="detail-value">{{ detail.node.weight ?? 100 }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">节点 IP</div>
          <div class="detail-value mono">{{ detail.node.ip || '—' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">创建时间</div>
          <div class="detail-value">{{ formatDateTime(detail.node.created_at) }}</div>
        </div>
        <div class="detail-item span-2">
          <div class="detail-label">节点地址</div>
          <div class="detail-value mono">{{ detail.node.agent_url || '未配置' }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">最后心跳</div>
          <div class="detail-value">{{ formatDateTime(detail.node.last_heartbeat) }}</div>
        </div>
        <div class="detail-item">
          <div class="detail-label">资源占用</div>
          <div class="detail-value">
            CPU {{ detail.node.cpu_usage != null ? `${detail.node.cpu_usage.toFixed(0)}%` : '—' }}
            · MEM {{ detail.node.memory_usage != null ? `${detail.node.memory_usage.toFixed(0)}%` : '—' }}
          </div>
        </div>
      </div>

      <div class="section-title">节点令牌</div>
      <div class="copy-row">
        <input class="input mono" :value="detail.token" readonly />
        <button class="btn btn-default" type="button" @click="copy(detail.token, '令牌')">
          <AppIcon name="copy" :size="15" />
          复制
        </button>
      </div>

      <div class="section-title">edge.env 配置文件</div>
      <div v-if="!detailProfileStored" class="alert alert-info">
        <AppIcon name="info" :size="16" class="icon" />
        <span>
          该节点没有硬件画像（未通过「添加节点」向导接入），下面按默认画像生成。
          可到「设置 → 硬件画像」调整并保存。
        </span>
      </div>
      <div v-if="detailListenerWarning || detailCallbackWarning" class="alert alert-warning">
        <AppIcon name="alert" :size="16" class="icon" />
        <span>{{ detailListenerWarning || detailCallbackWarning }}</span>
      </div>
      <div class="env-head">
        <span class="mono env-hardware">{{ detailHardwareSummary }}</span>
        <span class="spacer" />
        <button class="btn btn-text btn-sm" type="button" @click="copy(detailEnv, 'edge.env')">
          <AppIcon name="copy" :size="14" />
          复制
        </button>
        <button class="btn btn-text btn-sm" type="button" @click="downloadDetailEnv">
          下载 edge.env
        </button>
      </div>
      <div class="command-box env-box">
        <pre class="mono">{{ detailEnv }}</pre>
      </div>

      <div v-if="detailPlan.warnings.length" class="alert alert-warning">
        <AppIcon name="alert" :size="16" class="icon" />
        <div class="alert-list">
          <span v-for="w in detailPlan.warnings" :key="w">{{ w }}</span>
        </div>
      </div>
    </div>
  </div>
  <div v-else class="card card-body text-muted">加载中…</div>
</template>

<style scoped>
.form-col {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px 18px;
}

.detail-item.span-2 {
  grid-column: span 2;
}

.detail-label {
  font-size: 12px;
  color: var(--text-3);
}

.detail-value {
  margin-top: 3px;
  font-size: 13.5px;
  color: var(--text);
  word-break: break-all;
}

.section-title {
  margin-top: 4px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--divider);
  font-size: 13px;
  font-weight: 500;
  color: var(--text-2);
}

.copy-row {
  display: flex;
  gap: 8px;
}

.copy-row .input {
  flex: 1;
  font-size: 13px;
}

.env-head {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}

.env-hardware {
  font-size: 11.5px;
  color: var(--text-3);
}

.command-box {
  position: relative;
  background: #0f172a;
  border-radius: var(--radius);
  padding: 14px 16px;
  overflow-x: auto;
}

.command-box.env-box {
  max-height: 360px;
  overflow: auto;
}

.command-box pre {
  margin: 0;
  color: #d6e8ff;
  font-size: 12.5px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-all;
}

.alert-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

@media (max-width: 640px) {
  .detail-grid {
    grid-template-columns: 1fr;
  }

  .detail-item.span-2 {
    grid-column: span 1;
  }
}
</style>
