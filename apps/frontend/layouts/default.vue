<script setup lang="ts">
interface MenuSubItem {
  label: string
  to: string
  admin?: boolean
  separator?: boolean
}

interface MenuModule {
  code: string
  label: string
  to: string
  icon: string
  admin?: boolean
  subItems?: MenuSubItem[]
}

const MENU: MenuModule[] = [
  {
    code: 'dashboard',
    label: '看板',
    to: '/',
    icon: 'dashboard',
  },
  {
    code: 'servers',
    label: '网站',
    to: '/domains',
    icon: 'domain',
    subItems: [
      { label: '网站列表', to: '/domains' },
      { label: '访问日志', to: '/access-logs' },
      { label: '证书管理', to: '/certs' },
    ],
  },
  {
    code: 'cache',
    label: '缓存',
    to: '/rules',
    icon: 'rules',
    subItems: [
      { label: '缓存规则', to: '/rules' },
      { label: '默认缓存条件', to: '/cache-policies' },
      { label: '缓存刷新', to: '/purge' },
      { label: '缓存预热', to: '/prefetch' },
      { label: '缓存诊断', to: '/cache-debug' },
    ],
  },
  {
    code: 'security',
    label: '安全',
    to: '/ip-lists',
    icon: 'shield',
    subItems: [
      // WAF 细则挂在「网站 → 站点 → WAF」（对齐 GoEdge）；此处仅全局能力。
      { label: 'IP 名单', to: '/ip-lists' },
    ],
  },
  {
    code: 'clusters',
    label: '节点',
    to: '/nodes',
    icon: 'node',
    admin: true,
    subItems: [
      { label: '边缘节点', to: '/nodes' },
      { label: '远程安装', to: '/nodes/remote', admin: true },
      { label: 'L2 节点', to: '/l2-nodes', admin: true },
      { label: '缓存策略', to: '/cache-storage', admin: true },
      { label: 'SSH 凭证', to: '/ssh-grants', admin: true },
    ],
  },
  {
    code: 'dns',
    label: '智能 DNS',
    to: '/dns/zones',
    icon: 'globe',
    admin: true,
    subItems: [
      { label: '域名解析', to: '/dns/zones', admin: true },
      { label: 'DNS 服务商', to: '/dns/providers', admin: true },
      { label: 'DNS 节点', to: '/dns/nodes', admin: true },
      { label: '自定义线路', to: '/dns/lines', admin: true },
      { label: 'CDN 主域', to: '/dns/settings', admin: true },
    ],
  },
  {
    code: 'observe',
    label: '监控',
    to: '/stats',
    icon: 'stats',
    subItems: [
      { label: '大屏监控', to: '/monitor' },
      { label: '告警中心', to: '/alerts' },
    ],
  },
  {
    code: 'users',
    label: '用户',
    to: '/projects',
    icon: 'users',
    subItems: [
      { label: '用户管理', to: '/users', admin: true },
      { label: '用户组', to: '/groups', admin: true },
    ],
  },
  {
    code: 'log',
    label: '日志',
    to: '/login-logs',
    icon: 'audit',
    subItems: [
      { label: '操作审计', to: '/audit', admin: true },
    ],
  },
  {
    code: 'settings',
    label: '设置',
    to: '/settings',
    icon: 'settings',
    subItems: [
      { label: '基本设置', to: '/settings' },
      { label: '高级设置', to: '/settings/advanced', admin: true },
      { label: '配置发布', to: '/config-releases', admin: true },
    ],
  },
]

const route = useRoute()
const auth = useAuthStore()
const theme = useTheme()
const i18n = useLocale()

const NAV_KEYS: Record<string, string> = {
  '/': 'nav.dashboard',
  '/domains': 'nav.domains',
  '/domains/onboard': 'nav.onboard',
  '/rules': 'nav.rules',
  '/cache-policies': 'nav.cacheTemplates',
  '/cache-storage': 'nav.cacheStorage',
  '/cache-debug': 'nav.cacheDebug',
  '/certs': 'nav.certs',
  '/purge': 'nav.purge',
  '/prefetch': 'nav.prefetch',
  '/config-releases': 'nav.releases',
  '/ip-lists': 'nav.ipLists',
  '/nodes/remote': 'nav.nodeRemote',
  '/dns/settings': 'nav.dnsSettings',
  '/dns/zones': 'nav.dnsZones',
  '/dns/providers': 'nav.dnsProviders',
  '/dns/nodes': 'nav.dnsNodes',
  '/dns/lines': 'nav.dnsLines',
  '/settings/advanced': 'nav.settingsAdvanced',
  '/nodes': 'nav.nodes',
  '/l2-nodes': 'nav.l2Nodes',
  '/ssh-grants': 'nav.sshGrants',
  '/nodes/onboard': 'nav.nodeOnboard',
  '/stats': 'nav.stats',
  '/monitor': 'nav.monitor',
  '/alerts': 'nav.alerts',
  '/access-logs': 'nav.logs',
  '/audit': 'nav.audit',
  '/login-logs': 'nav.loginLogs',
  '/projects': 'nav.projects',
  '/users': 'nav.users',
  '/groups': 'nav.groups',
  '/dns': 'nav.dns',
  '/settings': 'nav.settingsBasic',
}

const MODULE_KEYS: Record<string, string> = {
  dashboard: 'module.dashboard',
  servers: 'module.servers',
  cache: 'module.cache',
  security: 'module.security',
  clusters: 'module.clusters',
  dns: 'module.dns',
  observe: 'module.observe',
  users: 'module.users',
  log: 'module.log',
  settings: 'module.settings',
}

const sidebarOpen = ref(false)
const sidebarCollapsed = ref(false)
const userMenuOpen = ref(false)
/** Manually expanded module codes (in addition to the route-active module). */
const expandedModules = ref<Set<string>>(new Set())

function toggleSidebarCollapsed() {
  sidebarCollapsed.value = !sidebarCollapsed.value
  if (import.meta.client) {
    localStorage.setItem('lagran:sidebar-collapsed', sidebarCollapsed.value ? '1' : '0')
  }
}

function isExpanded(mod: { code: string; subItems: unknown[] }): boolean {
  if (!mod.subItems.length) return false
  return isModuleActive(mod as MenuModule) || expandedModules.value.has(mod.code)
}

function toggleModule(mod: {
  code: string
  to: string
  subItems: { to: string; separator?: boolean }[]
}) {
  if (!mod.subItems.length) {
    navigateTo(mod.to)
    closeSidebar()
    return
  }
  // Collapsed rail: jump to the module landing leaf (no room for sub-list).
  if (sidebarCollapsed.value) {
    navigateTo(mod.to)
    closeSidebar()
    return
  }
  // Directory row only expands/collapses — never navigates by itself.
  const next = new Set(expandedModules.value)
  if (next.has(mod.code) && !isModuleActive(mod as MenuModule)) {
    next.delete(mod.code)
  } else {
    next.add(mod.code)
  }
  expandedModules.value = next
}

const NAV_PERMISSIONS: Record<string, string> = {
  '/domains': 'domain:read',
  '/domains/onboard': 'domain:write',
  '/rules': 'cache:read',
  '/cache-policies': 'cache:read',
  '/cache-storage': 'node:read',
  '/cache-debug': 'cache:read',
  '/certs': 'cert:read',
  '/purge': 'cache:write',
  '/prefetch': 'prefetch:read',
  '/config-releases': 'config:read',
  '/ip-lists': 'waf:read',
  '/nodes': 'node:read',
  '/l2-nodes': 'node:read',
  '/ssh-grants': 'node:read',
  '/nodes/onboard': 'node:write',
  '/nodes/remote': 'node:write',
  '/stats': 'analytics:read',
  '/monitor': 'analytics:read',
  '/alerts': 'alert:read',
  '/access-logs': 'analytics:read',
  '/audit': 'audit:read',
  '/login-logs': 'analytics:read',
  '/projects': 'project:read',
  '/users': 'user:read',
  '/groups': 'user:read',
  '/dns': 'settings:read',
  '/settings': 'settings:read',
}

function canAccess(to: string, admin?: boolean): boolean {
  if (admin && !auth.isAdmin) return false
  const permission = NAV_PERMISSIONS[to]
  return !permission || auth.can(permission)
}

function pathMatches(to: string, path: string): boolean {
  if (to === '/') return path === '/'
  return path === to || path.startsWith(`${to}/`)
}

/** Paths that belong to a module even when they are not the module's own `to`. */
const MODULE_PATHS: Record<string, string[]> = {
  servers: ['/domains', '/access-logs', '/certs'],
  cache: ['/rules', '/cache-policies', '/cache-debug', '/purge', '/prefetch'],
  security: ['/ip-lists'],
  clusters: [
    '/nodes',
    '/nodes/onboard',
    '/nodes/remote',
    '/l2-nodes',
    '/cache-storage',
    '/ssh-grants',
  ],
  dns: ['/dns'],
  observe: ['/stats', '/monitor', '/alerts'],
  users: ['/users', '/groups', '/projects'],
  log: ['/audit', '/login-logs'],
  settings: ['/settings', '/config-releases'],
  dashboard: ['/'],
}

function moduleOwnsPath(mod: MenuModule, path: string): boolean {
  const paths = MODULE_PATHS[mod.code] || [mod.to]
  return paths.some((p) => pathMatches(p, path))
}

function isModuleActive(mod: MenuModule): boolean {
  return moduleOwnsPath(mod, route.path)
}

function isSubActive(to: string): boolean {
  if (!to) return false
  if (to === '/domains') {
    if (route.path === '/domains') return true
    if (route.path.startsWith('/domains/onboard')) return false
    return route.path.startsWith('/domains/')
  }
  if (to === '/nodes') {
    if (route.path === '/nodes') return true
    if (route.path.startsWith('/nodes/onboard') || route.path.startsWith('/nodes/remote')) return false
    return route.path.startsWith('/nodes/')
  }
  if (to === '/l2-nodes') {
    return route.path.startsWith('/l2-nodes') || route.path.startsWith('/shield-nodes')
  }
  // Exact leaf so /settings does not also match /settings/advanced,
  // and /dns/zones does not light up every DNS tab.
  if (to === '/settings' || to === '/dns/settings' || to === '/dns/zones' || to === '/dns/providers' || to === '/dns/nodes' || to === '/dns/lines') {
    return route.path === to || route.path === `${to}/`
  }
  return pathMatches(to, route.path)
}

const modules = computed(() =>
  MENU
    .filter((mod) => canAccess(mod.to, mod.admin))
    .map((mod) => {
      const subItems = (mod.subItems || [])
        .filter((item) => item.separator || canAccess(item.to, item.admin))
        .map((item) => ({
          ...item,
          label: item.separator ? '-' : i18n.t(NAV_KEYS[item.to] || item.label),
        }))
        // Drop trailing / leading / duplicate separators after permission filter
        .filter((item, i, arr) => {
          if (!item.separator) return true
          if (i === 0 || i === arr.length - 1) return false
          return !arr[i - 1]?.separator
        })
      return {
        ...mod,
        label: i18n.t(MODULE_KEYS[mod.code] || mod.label),
        subItems,
      }
    }),
)

const brand = useBrand()

const DOMAIN_SECTION_LABELS: Record<string, string> = {
  settings: '基础信息',
  hostnames: '域名绑定',
  origins: '源站',
  certs: 'HTTPS / 证书',
  rules: '缓存规则',
  waf: 'WAF',
  advanced: '高级设置',
  stats: '统计',
  logs: '访问日志',
  delete: '删除',
}

const NODE_SECTION_LABELS: Record<string, string> = {
  detail: '概览',
  settings: '设置',
  profile: '硬件画像',
  install: '安装',
  delete: '删除',
}

const current = computed<{ group: string; label: string }>(() => {
  const path = route.path
  if (path === '/domains/onboard') {
    return { group: i18n.t('module.servers'), label: i18n.t('nav.onboard') }
  }
  if (path.startsWith('/domains/')) {
    const parts = path.split('/').filter(Boolean)
    // /domains/:id/:section?
    const section = parts[2] || 'settings'
    return {
      group: i18n.t('module.servers'),
      label: DOMAIN_SECTION_LABELS[section] || i18n.t('nav.domains'),
    }
  }
  if (path.startsWith('/nodes/remote')) {
    return { group: i18n.t('module.clusters'), label: i18n.t('nav.nodeRemote') }
  }
  if (path === '/nodes/onboard' || path.startsWith('/nodes/onboard')) {
    return { group: i18n.t('module.clusters'), label: i18n.t('nav.nodeOnboard') }
  }
  if (path.startsWith('/nodes/') && !path.startsWith('/nodes/onboard') && !path.startsWith('/nodes/remote')) {
    const parts = path.split('/').filter(Boolean)
    const section = parts[2] || 'detail'
    return {
      group: i18n.t('module.clusters'),
      label: NODE_SECTION_LABELS[section] || i18n.t('nav.nodes'),
    }
  }
  if (path.startsWith('/shield-nodes') || path.startsWith('/l2-nodes')) {
    return { group: i18n.t('module.clusters'), label: i18n.t('nav.l2Nodes') }
  }

  for (const mod of modules.value) {
    for (const sub of mod.subItems) {
      if (!sub.separator && isSubActive(sub.to)) {
        return { group: mod.label, label: sub.label }
      }
    }
    if (isModuleActive(mod) && (!mod.subItems.length || pathMatches(mod.to, path))) {
      return { group: mod.label, label: i18n.t(NAV_KEYS[mod.to] || mod.label) }
    }
  }

  for (const mod of modules.value) {
    if (isModuleActive(mod)) {
      return { group: mod.label, label: i18n.t(NAV_KEYS[mod.to] || mod.label) }
    }
  }

  return { group: i18n.t('module.dashboard'), label: brand.systemName.value }
})

useHead(() => ({ title: `${current.value.label} · ${brand.systemName.value}` }))

const displayName = computed(() => auth.user?.name || auth.user?.email || '未登录')
const initial = computed(() => (displayName.value || brand.mark.value).slice(0, 1).toUpperCase())

/** Topbar primary CTA follows the active module — never a global「添加站点」. */
const topbarCreate = computed(() => {
  const path = route.path
  if (path.startsWith('/nodes') && !path.startsWith('/nodes/onboard') && auth.can('node:write')) {
    return { to: '/nodes/onboard', label: '添加节点' }
  }
  if (
    (path === '/' || path.startsWith('/domains') || path.startsWith('/access-logs') || path.startsWith('/certs'))
    && auth.can('domain:write')
  ) {
    return { to: '/domains/onboard', label: '添加站点' }
  }
  return null
})

function closeSidebar() {
  sidebarOpen.value = false
}

function logout() {
  auth.logout()
  userMenuOpen.value = false
  navigateTo('/login')
}

watch(
  () => route.path,
  () => {
    sidebarOpen.value = false
    userMenuOpen.value = false
  },
)

let authRefreshTimer: ReturnType<typeof setInterval> | undefined

onMounted(async () => {
  if (import.meta.client) {
    sidebarCollapsed.value = localStorage.getItem('lagran:sidebar-collapsed') === '1'
  }
  auth.hydrate()
  await Promise.all([auth.refresh(), brand.refresh()])
  authRefreshTimer = setInterval(() => {
    void auth.refresh()
  }, 60_000)
})

onUnmounted(() => {
  if (authRefreshTimer) clearInterval(authRefreshTimer)
})
</script>

<template>
  <div class="app-shell" :class="{ 'sidebar-collapsed': sidebarCollapsed }">
    <aside class="sidebar" :class="{ open: sidebarOpen, collapsed: sidebarCollapsed }">
      <div class="sidebar-brand">
        <div class="brand-mark">{{ brand.mark }}</div>
        <div class="brand-text">
          <span class="brand-name">{{ brand.systemName }}</span>
          <span class="brand-sub">管理控制台</span>
        </div>
      </div>

      <nav class="sidebar-nav">
        <div v-for="mod in modules" :key="mod.code" class="menu-module" :class="{ expanded: isExpanded(mod) }">
          <!-- Leaf modules (e.g. 看板) navigate; directories only expand. -->
          <NuxtLink
            v-if="!mod.subItems.length"
            :to="mod.to"
            class="menu-item menu-module-link"
            :class="{ active: isModuleActive(mod) }"
            :title="sidebarCollapsed ? mod.label : undefined"
            @click="closeSidebar"
          >
            <AppIcon :name="mod.icon" :size="18" class="icon" />
            <span class="menu-label">{{ mod.label }}</span>
          </NuxtLink>
          <button
            v-else
            type="button"
            class="menu-item menu-module-link menu-module-toggle"
            :class="{ active: isModuleActive(mod) }"
            :title="sidebarCollapsed ? mod.label : undefined"
            @click="toggleModule(mod)"
          >
            <AppIcon :name="mod.icon" :size="18" class="icon" />
            <span class="menu-label">{{ mod.label }}</span>
            <AppIcon
              v-if="!sidebarCollapsed"
              name="chevron-down"
              :size="14"
              class="module-chevron"
              :class="{ open: isExpanded(mod) }"
            />
          </button>

          <div v-if="mod.subItems.length && isExpanded(mod) && !sidebarCollapsed" class="menu-sub-items">
            <template v-for="(sub, idx) in mod.subItems" :key="sub.separator ? `sep-${idx}` : sub.to">
              <div v-if="sub.separator" class="menu-separator" />
              <NuxtLink
                v-else
                :to="sub.to"
                class="menu-item menu-sub-item"
                :class="{ active: isSubActive(sub.to) }"
                @click="closeSidebar"
              >
                <AppIcon name="chevron-right" :size="12" class="sub-chevron" :class="{ visible: isSubActive(sub.to) }" />
                <span class="menu-label">{{ sub.label }}</span>
              </NuxtLink>
            </template>
          </div>
        </div>
      </nav>
    </aside>

    <div v-if="sidebarOpen" class="sidebar-mask" @click="closeSidebar" />

    <div class="main">
      <header class="topbar">
        <button class="btn btn-icon btn-default topbar-menu-btn" type="button" aria-label="打开导航"
          @click="sidebarOpen = !sidebarOpen">
          <AppIcon name="menu" :size="18" />
        </button>
        <button class="btn btn-icon btn-default sidebar-toggle-btn" type="button"
          :title="sidebarCollapsed ? '展开导航' : '收起导航'" @click="toggleSidebarCollapsed">
          <AppIcon name="menu" :size="18" />
        </button>

        <div class="crumbs">
          <span>控制台</span>
          <AppIcon name="chevron-right" :size="12" class="sep" />
          <span>{{ current.group }}</span>
          <AppIcon name="chevron-right" :size="12" class="sep" />
          <span class="current">{{ current.label }}</span>
        </div>

        <div class="topbar-right">
          <NuxtLink
            v-if="topbarCreate"
            :to="topbarCreate.to"
            class="btn btn-primary btn-sm topbar-create"
          >
            <AppIcon name="plus" :size="14" />
            {{ topbarCreate.label }}
          </NuxtLink>
          <button class="btn btn-icon btn-default" type="button" :title="i18n.t('common.theme')"
            @click="theme.toggle()">
            <AppIcon :name="theme.mode.value === 'dark' ? 'sun' : 'moon'" :size="16" />
          </button>

          <div class="user-chip" @click.stop="userMenuOpen = !userMenuOpen">
            <span class="avatar">{{ initial }}</span>
            <span class="user-name">{{ displayName }}</span>
            <AppIcon name="chevron-down" :size="14" />
          </div>

          <div v-if="userMenuOpen" class="user-dropdown" @click.stop>
            <div class="dropdown-head">
              <div class="dropdown-name">
                {{ displayName }}
                <span class="tag" :class="auth.isAdmin ? 'tag-info' : 'tag-neutral'" style="margin-left: 6px">
                  {{ auth.isAdmin ? '管理员' : '普通用户' }}
                </span>
              </div>
              <div class="dropdown-mail">{{ auth.user?.email }}</div>
            </div>
            <div class="dropdown-item" @click="navigateTo('/settings')">
              <AppIcon name="settings" :size="16" />
              个人设置
            </div>
            <div class="dropdown-item danger" @click="logout">
              <AppIcon name="logout" :size="16" />
              {{ i18n.t('common.logout') }}
            </div>
          </div>
        </div>
      </header>

      <div v-if="userMenuOpen" class="dropdown-backdrop" @click="userMenuOpen = false" />

      <main class="content">
        <slot />
      </main>

      <footer class="console-footer">
        <span>© {{ new Date().getFullYear() }} {{ brand.systemName }}</span>
      </footer>
    </div>

    <ToastHost />
    <ConfirmHost />
  </div>
</template>

<style scoped>
.dropdown-backdrop {
  position: fixed;
  inset: 0;
  z-index: 10;
}

.menu-module-toggle {
  width: 100%;
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
  font: inherit;
  color: inherit;
}

.module-chevron {
  margin-left: auto;
  opacity: 0.55;
  transition: transform 0.15s ease;
  flex: none;
}

.module-chevron.open {
  transform: rotate(180deg);
}

.menu-sub-items {
  padding: 2px 0 8px 0;
}

.menu-sub-item {
  padding-left: 28px !important;
  font-size: 13px !important;
  gap: 6px !important;
}

.menu-sub-item .sub-chevron {
  opacity: 0;
  flex: none;
}

.menu-sub-item .sub-chevron.visible,
.menu-sub-item.active .sub-chevron {
  opacity: 1;
}

.menu-separator {
  height: 1px;
  margin: 6px 14px;
  background: rgba(255, 255, 255, 0.08);
}
</style>
