<script setup lang="ts">
const route = useRoute()
const id = computed(() => String(route.params.id || ''))
const ctx = useNodeDetail(id)

provide('nodeId', id)
provide('nodeDetail', ctx)

const {
  detail,
  notFound,
  loading,
  upgrading,
  load,
  toggle,
  upgrade,
  statusLabel,
  statusTone,
} = ctx

const TABS = [
  { key: 'detail', label: '详情', icon: 'node' },
  { key: 'settings', label: '设置', icon: 'settings' },
  { key: 'install', label: '安装', icon: 'zap' },
  { key: 'delete', label: '删除', icon: 'trash' },
]

const SETTINGS_SECTIONS = [
  { key: 'settings', label: '基本信息', to: 'settings' },
  { key: 'profile', label: '硬件画像', to: 'profile' },
]

const sectionKey = computed(() => {
  const path = route.path
  const base = `/nodes/${id.value}`
  if (path === base || path === `${base}/`) return 'detail'
  return path.slice(base.length + 1).split('/')[0] || 'detail'
})

const activeTab = computed(() => {
  if (['detail', 'install', 'delete'].includes(sectionKey.value)) return sectionKey.value
  return 'settings'
})

const showLeftMenu = computed(() => activeTab.value === 'settings')

function tabTo(key: string) {
  return `/nodes/${id.value}/${key}`
}

watch(id, () => load(), { immediate: true })
</script>

<template>
  <div class="page node-detail">
    <div class="page-head">
      <div>
        <div class="page-title">
          <NuxtLink to="/nodes" class="btn btn-icon btn-default" aria-label="返回节点列表">
            <AppIcon name="arrow-left" :size="16" />
          </NuxtLink>
          <span>{{ detail?.node?.name || '节点详情' }}</span>
          <StatusTag v-if="detail?.node" :tone="statusTone(detail.node.status)" dot>
            {{ statusLabel(detail.node.status) }}
          </StatusTag>
        </div>
        <div class="page-desc">节点详情 · 对齐 GoEdge「节点 → 详情 / 设置 / 安装」</div>
      </div>
      <div class="page-actions">
        <button
          v-if="detail"
          class="btn btn-default"
          type="button"
          :disabled="upgrading"
          @click="upgrade()"
        >
          {{ upgrading ? '升级中…' : '升级' }}
        </button>
        <button
          v-if="detail"
          class="btn"
          :class="detail.node.status === 'disabled' ? 'btn-primary' : 'btn-default'"
          type="button"
          @click="toggle()"
        >
          {{ detail.node.status === 'disabled' ? '启用节点' : '停用节点' }}
        </button>
        <button class="btn btn-default" type="button" :disabled="loading" @click="load()">
          <AppIcon name="refresh" :size="15" />
          刷新
        </button>
      </div>
    </div>

    <div v-if="notFound" class="card">
      <EmptyState icon="node" title="节点不存在" text="该节点可能已被删除，或你没有访问权限。">
        <NuxtLink to="/nodes" class="btn btn-default btn-sm">返回节点列表</NuxtLink>
      </EmptyState>
    </div>

    <div v-else-if="loading && !detail" class="card">
      <div class="card-body text-muted">正在加载节点…</div>
    </div>

    <template v-else>
      <nav class="domain-tabs">
        <NuxtLink
          v-for="tab in TABS"
          :key="tab.key"
          :to="tabTo(tab.key)"
          class="domain-tab"
          :class="{ active: activeTab === tab.key, danger: tab.key === 'delete' }"
        >
          <AppIcon :name="tab.icon" :size="14" />
          {{ tab.label }}
        </NuxtLink>
      </nav>

      <div class="domain-body" :class="{ 'with-left': showLeftMenu }">
        <aside v-if="showLeftMenu" class="domain-left">
          <NuxtLink
            v-for="sec in SETTINGS_SECTIONS"
            :key="sec.key"
            :to="tabTo(sec.to)"
            class="domain-left-item"
            :class="{ active: sectionKey === sec.key }"
          >
            <AppIcon name="chevron-right" :size="11" class="chev" :class="{ on: sectionKey === sec.key }" />
            {{ sec.label }}
          </NuxtLink>
        </aside>
        <div class="domain-main">
          <NuxtPage />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.domain-tabs {
  display: flex;
  gap: 2px;
  margin-bottom: 16px;
  border-bottom: 1px solid var(--border);
}

.domain-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 10px 16px;
  color: var(--text-2);
  font-size: 13.5px;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  text-decoration: none;
}

.domain-tab:hover {
  color: var(--text);
}

.domain-tab.active {
  color: var(--brand);
  border-bottom-color: var(--brand);
  font-weight: 500;
}

.domain-tab.danger.active {
  color: var(--danger);
  border-bottom-color: var(--danger);
}

.domain-body.with-left {
  display: grid;
  grid-template-columns: 180px minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}

.domain-left {
  position: sticky;
  top: 12px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 0;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg, 10px);
  background: var(--card);
}

.domain-left-item {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 9px 12px;
  color: var(--text-2);
  font-size: 13px;
  text-decoration: none;
}

.domain-left-item:hover {
  color: var(--text);
  background: var(--bg);
}

.domain-left-item.active {
  color: var(--brand);
  background: color-mix(in srgb, var(--brand) 8%, var(--card));
  font-weight: 500;
}

.domain-left-item .chev {
  opacity: 0;
  flex: none;
}

.domain-left-item .chev.on,
.domain-left-item.active .chev {
  opacity: 1;
}

.domain-main {
  min-width: 0;
}

@media (max-width: 860px) {
  .domain-body.with-left {
    grid-template-columns: 1fr;
  }

  .domain-left {
    position: static;
    flex-direction: row;
    flex-wrap: wrap;
    padding: 6px;
  }
}
</style>
