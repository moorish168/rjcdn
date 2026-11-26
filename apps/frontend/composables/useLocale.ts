type Locale = 'zh' | 'en'

const DICT: Record<string, { zh: string; en: string }> = {
  'module.dashboard': { zh: '看板', en: 'Dashboard' },
  'module.servers': { zh: '网站', en: 'Sites' },
  'module.cache': { zh: '缓存', en: 'Cache' },
  'module.security': { zh: '安全', en: 'Security' },
  'module.clusters': { zh: '节点', en: 'Nodes' },
  'module.dns': { zh: '智能 DNS', en: 'Smart DNS' },
  'module.observe': { zh: '监控', en: 'Monitor' },
  'module.users': { zh: '用户', en: 'Users' },
  'module.log': { zh: '日志', en: 'Logs' },
  'module.settings': { zh: '设置', en: 'Settings' },
  // legacy group keys kept for any leftover references
  'group.overview': { zh: '看板', en: 'Dashboard' },
  'group.edge': { zh: '网站', en: 'Sites' },
  'group.nodes': { zh: '节点', en: 'Nodes' },
  'group.observe': { zh: '监控', en: 'Monitor' },
  'group.system': { zh: '设置', en: 'Settings' },
  'nav.dashboard': { zh: '看板', en: 'Dashboard' },
  'nav.domains': { zh: '网站列表', en: 'Site List' },
  'nav.onboard': { zh: '添加站点', en: 'Add site' },
  'nav.rules': { zh: '缓存规则', en: 'Cache Rules' },
  'nav.cacheTemplates': { zh: '默认缓存条件', en: 'Default Cache Conditions' },
  'nav.cachePolicies': { zh: '默认缓存条件', en: 'Default Cache Conditions' },
  'nav.cacheStorage': { zh: '缓存策略', en: 'Cache Storage' },
  'nav.cacheDebug': { zh: '缓存诊断', en: 'Cache Debug' },
  'nav.waf': { zh: 'WAF 防护', en: 'WAF' },
  'nav.ipLists': { zh: 'IP 名单', en: 'IP Lists' },
  'nav.wafEvents': { zh: 'WAF 事件', en: 'WAF Events' },
  'nav.certs': { zh: '证书管理', en: 'Certificates' },
  'nav.purge': { zh: '缓存刷新', en: 'Purge' },
  'nav.prefetch': { zh: '缓存预热', en: 'Prefetch' },
  'nav.releases': { zh: '配置发布', en: 'Releases' },
  'nav.nodes': { zh: '边缘节点', en: 'Edge Nodes' },
  'nav.l2Nodes': { zh: 'L2 节点', en: 'L2 Nodes' },
  'nav.sshGrants': { zh: 'SSH 凭证', en: 'SSH Credentials' },
  'nav.nodeOnboard': { zh: '添加节点', en: 'Add Edge Wizard' },
  'nav.nodeRemote': { zh: '远程安装', en: 'Remote Install' },
  'nav.stats': { zh: '统计分析', en: 'Analytics' },
  'nav.monitor': { zh: '大屏监控', en: 'Monitor Wall' },
  'nav.alerts': { zh: '告警中心', en: 'Alerts' },
  'nav.logs': { zh: '访问日志', en: 'Access Logs' },
  'nav.audit': { zh: '操作审计', en: 'Audit Log' },
  'nav.loginLogs': { zh: '登录日志', en: 'Login Log' },
  'nav.projects': { zh: '项目管理', en: 'Projects' },
  'nav.users': { zh: '用户管理', en: 'Users' },
  'nav.groups': { zh: '用户组', en: 'Groups' },
  'nav.dns': { zh: '智能 DNS', en: 'Smart DNS' },
  'nav.dnsSettings': { zh: 'CDN 主域', en: 'CDN Main Domain' },
  'nav.dnsZones': { zh: '域名解析', en: 'DNS' },
  'nav.dnsProviders': { zh: 'DNS 服务商', en: 'DNS Providers' },
  'nav.dnsNodes': { zh: 'DNS 节点', en: 'DNS Nodes' },
  'nav.dnsLines': { zh: '自定义线路', en: 'Custom Lines' },
  'nav.settings': { zh: '设置', en: 'Settings' },
  'nav.settingsBasic': { zh: '基本设置', en: 'Basic Settings' },
  'nav.settingsAdvanced': { zh: '高级设置', en: 'Advanced Settings' },
  'nav.config': { zh: '配置发布', en: 'Config Releases' },
  'common.refresh': { zh: '刷新', en: 'Refresh' },
  'common.export': { zh: '导出 CSV', en: 'Export CSV' },
  'common.logout': { zh: '退出登录', en: 'Sign out' },
  'common.profile': { zh: '个人设置', en: 'Profile' },
  'common.language': { zh: 'English', en: '简体中文' },
  'common.theme': { zh: '切换深色模式', en: 'Toggle dark mode' },
  'common.online': { zh: '在线', en: 'Online' },
  'common.offline': { zh: '离线', en: 'Offline' },
  'diag.title': { zh: '域名诊断', en: 'Domain Diagnostics' },
  'diag.run': { zh: '开始诊断', en: 'Run diagnostics' },
  'diag.dns': { zh: 'DNS 是否正确', en: 'DNS' },
  'diag.cert': { zh: '证书是否有效', en: 'Certificate' },
  'diag.origin': { zh: '回源是否可达', en: 'Origin reachability' },
  'diag.cache': { zh: '缓存是否命中', en: 'Cache status' },
}

/** Small shell-level i18n dictionary (navigation and global controls). */
export function useLocale() {
  const locale = useState<Locale>('lagran_locale', () => 'zh')

  function t(key: string): string {
    const entry = DICT[key]
    if (!entry) return key
    return entry[locale.value]
  }

  function apply(next: Locale) {
    locale.value = next
    if (!import.meta.client) return
    document.documentElement.lang = next === 'zh' ? 'zh-CN' : 'en'
    try {
      localStorage.setItem('lagran_locale', next)
    } catch {
      // ignore
    }
  }

  function init() {
    if (!import.meta.client) return
    let saved: string | null = null
    try {
      saved = localStorage.getItem('lagran_locale')
    } catch {
      saved = null
    }
    apply(saved === 'en' ? 'en' : 'zh')
  }

  function toggle() {
    apply(locale.value === 'zh' ? 'en' : 'zh')
  }

  return { locale, t, apply, init, toggle }
}
