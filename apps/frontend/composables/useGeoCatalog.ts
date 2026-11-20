/**
 * Continent → country catalog shared by the DNS line and WAF 地区 pickers.
 *
 * Served by the control plane from the same GeoLite2-Country build the edges
 * load, so a continent picked here covers exactly the countries the edge can
 * report for it.
 */
export interface GeoCountry {
  code: string
  name_zh: string
  name_en: string
}

export interface GeoContinent {
  code: string
  name_zh: string
  name_en: string
  countries: GeoCountry[]
}

// One copy per tab (the console is a client-only SPA); the catalog never
// changes while the page is open.
const catalog = ref<GeoContinent[]>([])
let pending: Promise<void> | null = null

export function useGeoCatalog() {
  const api = useApi()

  async function load(): Promise<void> {
    if (catalog.value.length) return
    if (!pending) {
      pending = api
        .get<GeoContinent[]>('/api/v1/geo-catalog')
        .then((rows) => {
          catalog.value = rows || []
        })
        .catch(() => {
          catalog.value = []
        })
        .finally(() => {
          pending = null
        })
    }
    await pending
  }

  const countryIndex = computed(() => {
    const index = new Map<string, { name: string; continent: string }>()
    for (const cont of catalog.value) {
      for (const co of cont.countries) index.set(co.code, { name: co.name_zh, continent: cont.code })
    }
    return index
  })

  function continentCountries(code: string): string[] {
    return catalog.value.find((c) => c.code === code)?.countries.map((co) => co.code) || []
  }

  function continentName(code: string): string {
    return catalog.value.find((c) => c.code === code)?.name_zh || code
  }

  function countryName(code: string): string {
    return countryIndex.value.get(code)?.name || code
  }

  function continentOf(country: string): string {
    return countryIndex.value.get(country)?.continent || ''
  }

  /** Fold a flat country selection into whole continents plus the rest. */
  function collapse(selected: string[]): { continents: string[]; countries: string[] } {
    const set = new Set(selected)
    const continents = catalog.value
      .filter((c) => c.countries.length && c.countries.every((co) => set.has(co.code)))
      .map((c) => c.code)
    const covered = new Set(continents.flatMap(continentCountries))
    const countries = [...set].filter((code) => !covered.has(code)).sort()
    return { continents, countries }
  }

  /** Every country a set of whole continents plus single countries covers. */
  function expand(continents: string[], countries: string[]): string[] {
    const set = new Set<string>(countries)
    for (const cont of continents) for (const code of continentCountries(cont)) set.add(code)
    return [...set].sort()
  }

  /** e.g. 「亚洲（全部）、美国、加拿大 等 9 项」 */
  function areaLabel(continents: string[], countries: string[], max = 6): string {
    const names = [
      ...continents.map((c) => `${continentName(c)}（全部）`),
      ...countries.map(countryName),
    ]
    if (names.length > max) return `${names.slice(0, max).join('、')} 等 ${names.length} 项`
    return names.join('、')
  }

  return {
    catalog,
    load,
    continentCountries,
    continentName,
    countryName,
    continentOf,
    collapse,
    expand,
    areaLabel,
  }
}
