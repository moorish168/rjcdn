import type { GeoRegionDto } from '~/composables/useDnsConsole'

type ApiClient = {
  get: <T>(url: string) => Promise<T>
  post: <T>(url: string, body?: unknown) => Promise<T>
}

function parseCountriesCsv(raw: string | undefined | null): string[] {
  if (!raw) return []
  return raw
    .split(/[,;\s]+/)
    .map((s) => s.trim().toUpperCase())
    .filter((s) => /^[A-Z]{2}$/.test(s))
}

function sameCountrySet(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false
  const sa = [...a].sort()
  const sb = [...b].sort()
  return sa.every((c, i) => c === sb[i])
}

export function countriesFromDnsLine(
  l: GeoRegionDto | undefined | null,
  continentCountries: (code: string) => string[],
): string[] {
  if (!l) return []
  let countries = parseCountriesCsv(l.countries)
  if (!countries.length && l.continent) countries = continentCountries(l.continent)
  if (!countries.length && l.country) countries.push(l.country.toUpperCase())
  return [...new Set(countries)].sort()
}

/**
 * Resolve a geo_regions id for DNS line scheduling.
 * Prefer an explicit line id; otherwise find/create a line for the country set.
 */
export async function resolveDnsLineId(
  api: ApiClient,
  opts: {
    lineId?: string
    countries?: string[]
    note?: string
  },
): Promise<string> {
  const explicit = (opts.lineId || '').trim()
  if (explicit) return explicit
  const countries = opts.countries || []
  if (!countries.length) return ''
  const id = await ensureDnsLineForCountries(api, countries, opts.note)
  return id || ''
}

/** Find an existing line for this country set, or create a lightweight one. */
export async function ensureDnsLineForCountries(
  api: ApiClient,
  countries: string[],
  note = '由解析请求来源自动创建',
): Promise<string | null> {
  const picked = [
    ...new Set(countries.map((c) => c.trim().toUpperCase()).filter((c) => /^[A-Z]{2}$/.test(c))),
  ].sort()
  if (!picked.length) return null

  const geo = useGeoCatalog()
  await geo.load()
  const lines = (await api.get<GeoRegionDto[]>('/api/v1/geo-regions')) || []

  for (const l of lines) {
    if (!l.enabled || l.is_default) continue
    if (l.isp?.trim() || l.province?.trim() || l.city?.trim()) continue
    if (sameCountrySet(countriesFromDnsLine(l, geo.continentCountries), picked)) return l.id
  }

  const folded = geo.collapse(picked)
  const wholeContinent =
    folded.continents.length === 1 &&
    !folded.countries.length &&
    picked.length === geo.continentCountries(folded.continents[0]).length
  const name = geo.areaLabel(folded.continents, folded.countries, 6) || picked.join(',')
  const baseCode = wholeContinent
    ? `area-${folded.continents[0].toLowerCase()}`
    : `area-${picked.join('-').toLowerCase()}`.slice(0, 48)
  let code = baseCode
  let n = 2
  while (lines.some((l) => l.code === code)) {
    code = `${baseCode}-${n++}`.slice(0, 56)
  }
  const created = await api.post<GeoRegionDto>('/api/v1/geo-regions', {
    code,
    name,
    kind: 'custom',
    country: wholeContinent ? '' : picked.length === 1 ? picked[0] : '',
    continent: wholeContinent
      ? folded.continents[0]
      : folded.continents.length === 1
        ? folded.continents[0]
        : '',
    countries: wholeContinent ? [] : picked,
    province: '',
    city: '',
    isp: '',
    is_default: false,
    enabled: true,
    note,
  })
  return created.id
}
