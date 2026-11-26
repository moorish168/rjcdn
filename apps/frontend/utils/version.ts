/**
 * Version for display: the release number only.
 *
 * Builds report `1.0.6+caa95a1` (commit hash, `-dirty` for local trees) and
 * release tags read `v1.0.7`; both show as `1.0.6` / `1.0.7`. A value without
 * a leading number is shown as received.
 */
export function displayVersion(raw?: string | null): string {
  const value = (raw ?? '').trim()
  const match = /^v?(\d+(?:\.\d+)*)/i.exec(value)
  return match ? match[1] : value
}
