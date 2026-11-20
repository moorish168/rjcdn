const DEFAULT_NAME = '控制台'

export function useBrand() {
  const systemName = useState<string>('console_system_name', () => DEFAULT_NAME)
  const loaded = useState<boolean>('console_system_name_loaded', () => false)

  async function refresh() {
    const api = useApi()
    try {
      const cfg = await api.get<{ system_name?: string }>('/api/v1/auth/config')
      const name = (cfg?.system_name || '').trim()
      systemName.value = name || DEFAULT_NAME
      loaded.value = true
    } catch {
      // Keep current / default name when offline (login page still renders).
      if (!loaded.value) systemName.value = DEFAULT_NAME
    }
  }

  async function save(name: string) {
    const api = useApi()
    const body = await api.put<{ system_name: string }>('/api/v1/system/branding', {
      system_name: name.trim(),
    })
    systemName.value = (body?.system_name || name).trim() || DEFAULT_NAME
    return systemName.value
  }

  const mark = computed(() => {
    const n = systemName.value.trim()
    return (n.charAt(0) || 'C').toUpperCase()
  })

  return { systemName, mark, loaded, refresh, save }
}
