/** Operator-facing site label; falls back to primary hostname. */
export function siteLabel(d: { display_name?: string | null; name?: string | null } | null | undefined): string {
  const label = String(d?.display_name || '').trim()
  if (label) return label
  return String(d?.name || '').trim() || '—'
}

/** Option text: "自定义名 (hostname)" when both differ. */
export function siteOptionLabel(d: { display_name?: string | null; name?: string | null } | null | undefined): string {
  const label = String(d?.display_name || '').trim()
  const name = String(d?.name || '').trim()
  if (label && name && label !== name) return `${label} (${name})`
  return label || name || '—'
}
