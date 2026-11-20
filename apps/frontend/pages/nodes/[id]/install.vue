<script setup lang="ts">
import type { useNodeDetail } from '~/composables/useNodeDetail'

const id = inject<Ref<string>>('nodeId')!
const ctx = inject('nodeDetail') as ReturnType<typeof useNodeDetail>
const { detail, detailEnv, copy, downloadDetailEnv } = ctx
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">安装节点</div>
        <div class="card-desc">对齐 GoEdge「安装节点」：远程 SSH 装机，或手动下发 edge.env</div>
      </div>
    </div>
    <div class="card-body">
      <div class="install-grid">
        <div class="install-card">
          <div class="install-title">远程安装（推荐）</div>
          <p class="text-muted">使用已保存的 SSH 凭证，由控制面在目标机器安装 Edge 并写入配置。</p>
          <div class="install-actions">
            <NuxtLink
              class="btn btn-primary btn-sm"
              :to="`/nodes/remote?node=${encodeURIComponent(id)}`"
            >
              打开远程安装
            </NuxtLink>
            <NuxtLink to="/ssh-grants" class="btn btn-default btn-sm">管理 SSH 凭证</NuxtLink>
          </div>
        </div>
        <div class="install-card">
          <div class="install-title">手动安装</div>
          <p class="text-muted">复制令牌与 edge.env，到目标机器启动 Edge 进程。</p>
          <div class="install-actions">
            <button
              class="btn btn-default btn-sm"
              type="button"
              :disabled="!detail?.token"
              @click="copy(detail?.token || '', '令牌')"
            >
              复制令牌
            </button>
            <button class="btn btn-default btn-sm" type="button" @click="copy(detailEnv, 'edge.env')">
              复制 edge.env
            </button>
            <button class="btn btn-default btn-sm" type="button" @click="downloadDetailEnv">
              下载 edge.env
            </button>
          </div>
        </div>
      </div>

      <div class="section-title" style="margin-top: 18px">当前节点</div>
      <div class="kv-list">
        <div class="kv">
          <span class="kv-key">名称</span>
          <span class="kv-val">{{ detail?.node?.name || '—' }}</span>
        </div>
        <div class="kv">
          <span class="kv-key">公网 IP</span>
          <span class="kv-val mono">{{ detail?.node?.public_ip || '未配置' }}</span>
        </div>
        <div class="kv">
          <span class="kv-key">Agent</span>
          <span class="kv-val mono">{{ detail?.node?.agent_url || '未配置' }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.install-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.install-card {
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg, 10px);
  background: var(--bg);
}

.install-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
  margin-bottom: 6px;
}

.install-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
}

.section-title {
  padding-bottom: 6px;
  border-bottom: 1px solid var(--divider);
  font-size: 13px;
  font-weight: 500;
  color: var(--text-2);
}

.kv-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 12px;
}

.kv {
  display: flex;
  gap: 12px;
  font-size: 13px;
}

.kv-key {
  width: 72px;
  flex: none;
  color: var(--text-3);
}

.kv-val {
  color: var(--text-2);
  word-break: break-all;
}

@media (max-width: 720px) {
  .install-grid {
    grid-template-columns: 1fr;
  }
}
</style>
