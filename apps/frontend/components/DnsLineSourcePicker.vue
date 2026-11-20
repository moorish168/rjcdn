<script setup lang="ts">
/**
 * Aliyun-style「解析请求来源」cascader for DNS records.
 * Left: 默认 / 地域 / 运营商 / 自定义线路; right columns drill into options.
 */
import type { GeoRegionDto } from '~/composables/useDnsConsole'

const props = defineProps<{
  countries: string[]
  lineId: string
  lines: GeoRegionDto[]
}>()

const emit = defineEmits<{
  'update:countries': [value: string[]]
  'update:lineId': [value: string]
}>()

const geo = useGeoCatalog()
const open = ref(false)
const root = ref<HTMLElement | null>(null)

type Tab = 'default' | 'region' | 'isp' | 'custom'
const tab = ref<Tab>('default')
/** Under 地域: china | overseas */
const regionSide = ref<'china' | 'overseas'>('china')
/** Overseas continent being inspected for per-country tweaks (optional). */
const overseasCont = ref('')

onMounted(() => {
  void geo.load()
})

const selected = computed(() => new Set(props.countries.map((c) => c.toUpperCase())))

function allNonCn(): string[] {
  const out: string[] = []
  for (const cont of geo.catalog.value) {
    for (const co of cont.countries) {
      if (co.code !== 'CN') out.push(co.code)
    }
  }
  return out.sort()
}

function continentCodesExceptCn(code: string): string[] {
  return geo.continentCountries(code).filter((c) => c !== 'CN')
}

const ispLines = computed(() =>
  props.lines.filter((l) => l.enabled && !l.is_default && !!(l.isp || '').trim()),
)
const customLines = computed(() =>
  props.lines.filter(
    (l) => l.enabled && !l.is_default && !(l.isp || '').trim() && l.kind !== 'default',
  ),
)

const mode = computed<'default' | 'geo' | 'line'>(() => {
  if (props.lineId) {
    const l = props.lines.find((x) => x.id === props.lineId)
    if (l && !l.is_default) return 'line'
  }
  if (props.countries.length) return 'geo'
  return 'default'
})

watch(
  mode,
  (m) => {
    if (m === 'default') tab.value = 'default'
    else if (m === 'line') {
      const l = props.lines.find((x) => x.id === props.lineId)
      tab.value = l && (l.isp || '').trim() ? 'isp' : 'custom'
    } else tab.value = 'region'
  },
  { immediate: true },
)

const summary = computed(() => {
  if (mode.value === 'default') return '默认 / 默认'
  if (mode.value === 'line') {
    const l = props.lines.find((x) => x.id === props.lineId)
    const group = l && (l.isp || '').trim() ? '运营商' : '自定义线路'
    return `${group} / ${l?.name || '线路'}`
  }
  const set = selected.value
  if (set.size === 1 && set.has('CN')) return '地域 / 中国地区'
  const nonCn = allNonCn()
  if (nonCn.length && nonCn.every((c) => set.has(c)) && !set.has('CN')) {
    return '地域 / 境外'
  }
  const labels: string[] = []
  if (set.has('CN')) labels.push('中国地区')
  for (const cont of geo.catalog.value) {
    const codes = continentCodesExceptCn(cont.code)
    if (codes.length && codes.every((c) => set.has(c))) labels.push(cont.name_zh)
  }
  if (labels.length) {
    // If every selected country is covered by the labels above, stop here.
    const covered = new Set<string>()
    if (labels.includes('中国地区')) covered.add('CN')
    for (const cont of geo.catalog.value) {
      if (labels.includes(cont.name_zh)) {
        for (const c of continentCodesExceptCn(cont.code)) covered.add(c)
      }
    }
    if ([...set].every((c) => covered.has(c))) {
      return `地域 / ${labels.join('、')}`
    }
  }
  const folded = geo.collapse([...set])
  const area = geo.areaLabel(folded.continents, folded.countries, 4)
  return area ? `地域 / ${area}` : '地域 / —'
})

function pickDefault() {
  emit('update:countries', [])
  emit('update:lineId', '')
  tab.value = 'default'
}

function pickLine(id: string) {
  emit('update:lineId', id)
  emit('update:countries', [])
}

function setCountries(next: string[]) {
  emit('update:countries', [...new Set(next.map((c) => c.toUpperCase()))].sort())
  emit('update:lineId', '')
}

function toggleChina(on: boolean) {
  const next = new Set(selected.value)
  if (on) next.add('CN')
  else next.delete('CN')
  setCountries([...next])
}

function chinaChecked(): boolean {
  return selected.value.has('CN')
}

function overseasAllChecked(): boolean {
  const codes = allNonCn()
  return codes.length > 0 && codes.every((c) => selected.value.has(c))
}

function overseasAllIndeterminate(): boolean {
  const codes = allNonCn()
  const n = codes.filter((c) => selected.value.has(c)).length
  return n > 0 && n < codes.length
}

function toggleOverseasAll(on: boolean) {
  const next = new Set(selected.value)
  for (const c of allNonCn()) {
    if (on) next.add(c)
    else next.delete(c)
  }
  // Keep CN as-is when toggling 境外
  setCountries([...next])
}

function continentChecked(code: string): boolean {
  const codes = continentCodesExceptCn(code)
  return codes.length > 0 && codes.every((c) => selected.value.has(c))
}

function continentIndeterminate(code: string): boolean {
  const codes = continentCodesExceptCn(code)
  const n = codes.filter((c) => selected.value.has(c)).length
  return n > 0 && n < codes.length
}

function toggleContinent(code: string, on: boolean) {
  const next = new Set(selected.value)
  for (const c of continentCodesExceptCn(code)) {
    if (on) next.add(c)
    else next.delete(c)
  }
  setCountries([...next])
}

function toggleCountry(code: string, on: boolean) {
  const next = new Set(selected.value)
  if (on) next.add(code)
  else next.delete(code)
  setCountries([...next])
}

function onDocClick(e: MouseEvent) {
  if (!open.value) return
  if (root.value && !root.value.contains(e.target as Node)) open.value = false
}

watch(open, (v) => {
  if (v) document.addEventListener('mousedown', onDocClick)
  else document.removeEventListener('mousedown', onDocClick)
})

onBeforeUnmount(() => document.removeEventListener('mousedown', onDocClick))
</script>

<template>
  <div ref="root" class="dns-line-src" :class="{ open }">
    <button class="dns-line-src-trigger input" type="button" @click="open = !open">
      <span class="dns-line-src-value">{{ summary }}</span>
      <AppIcon name="chevron-right" :size="14" class="dns-line-src-caret" :class="{ open }" />
    </button>

    <div v-if="open" class="dns-line-src-panel">
      <div class="dns-line-src-col cats">
        <button
          type="button"
          class="dns-line-src-cat"
          :class="{ active: tab === 'default' }"
          @click="tab = 'default'"
        >
          默认
        </button>
        <button
          type="button"
          class="dns-line-src-cat"
          :class="{ active: tab === 'region' }"
          @click="tab = 'region'"
        >
          地域
        </button>
        <button
          type="button"
          class="dns-line-src-cat"
          :class="{ active: tab === 'isp' }"
          @click="tab = 'isp'"
        >
          运营商
        </button>
        <button
          type="button"
          class="dns-line-src-cat"
          :class="{ active: tab === 'custom' }"
          @click="tab = 'custom'"
        >
          自定义线路
        </button>
      </div>

      <!-- 默认 -->
      <div v-if="tab === 'default'" class="dns-line-src-col">
        <label class="dns-line-src-item check">
          <input type="checkbox" :checked="mode === 'default'" @change="pickDefault" />
          默认
        </label>
      </div>

      <!-- 地域：中国地区 / 境外 → 洲 -->
      <template v-else-if="tab === 'region'">
        <div class="dns-line-src-col">
          <div
            class="dns-line-src-item"
            :class="{ active: regionSide === 'china' }"
            @click="regionSide = 'china'"
          >
            <label class="check" @click.stop>
              <input type="checkbox" :checked="chinaChecked()" @change="toggleChina(($event.target as HTMLInputElement).checked)" />
              中国地区
            </label>
          </div>
          <div
            class="dns-line-src-item"
            :class="{ active: regionSide === 'overseas' }"
            @click="regionSide = 'overseas'"
          >
            <label class="check" @click.stop>
              <input
                type="checkbox"
                :checked="overseasAllChecked()"
                :indeterminate.prop="overseasAllIndeterminate()"
                @change="toggleOverseasAll(($event.target as HTMLInputElement).checked)"
              />
              境外
            </label>
          </div>
        </div>
        <div v-if="regionSide === 'china'" class="dns-line-src-col">
          <label class="dns-line-src-item check">
            <input type="checkbox" :checked="chinaChecked()" @change="toggleChina(($event.target as HTMLInputElement).checked)" />
            中国
          </label>
        </div>
        <div v-else class="dns-line-src-col wide">
          <label
            v-for="cont in geo.catalog.value"
            :key="cont.code"
            class="dns-line-src-item check"
            @mouseenter="overseasCont = cont.code"
          >
            <input
              type="checkbox"
              :checked="continentChecked(cont.code)"
              :indeterminate.prop="continentIndeterminate(cont.code)"
              @change="toggleContinent(cont.code, ($event.target as HTMLInputElement).checked)"
            />
            {{ cont.name_zh }}
          </label>
          <p v-if="!geo.catalog.value.length" class="dns-line-src-empty">正在加载地区目录…</p>
        </div>
        <div
          v-if="regionSide === 'overseas' && overseasCont && continentCodesExceptCn(overseasCont).length"
          class="dns-line-src-col wide"
        >
          <div class="dns-line-src-col-title">{{ geo.continentName(overseasCont) }}</div>
          <label
            v-for="code in continentCodesExceptCn(overseasCont)"
            :key="code"
            class="dns-line-src-item check"
          >
            <input
              type="checkbox"
              :checked="selected.has(code)"
              @change="toggleCountry(code, ($event.target as HTMLInputElement).checked)"
            />
            {{ geo.countryName(code) }}
          </label>
        </div>
      </template>

      <!-- 运营商 -->
      <div v-else-if="tab === 'isp'" class="dns-line-src-col wide">
        <label
          v-for="l in ispLines"
          :key="l.id"
          class="dns-line-src-item check"
        >
          <input
            type="checkbox"
            :checked="lineId === l.id"
            @change="($event.target as HTMLInputElement).checked ? pickLine(l.id) : pickDefault()"
          />
          {{ l.name }}
          <span v-if="l.isp" class="dns-line-src-meta">{{ l.isp }}</span>
        </label>
        <p v-if="!ispLines.length" class="dns-line-src-empty">
          暂无运营商线路。可在「自定义线路」里添加并填写运营商。
        </p>
      </div>

      <!-- 自定义线路 -->
      <div v-else class="dns-line-src-col wide">
        <label
          v-for="l in customLines"
          :key="l.id"
          class="dns-line-src-item check"
        >
          <input
            type="checkbox"
            :checked="lineId === l.id"
            @change="($event.target as HTMLInputElement).checked ? pickLine(l.id) : pickDefault()"
          />
          {{ l.name }}
        </label>
        <p v-if="!customLines.length" class="dns-line-src-empty">
          暂无自定义线路。可先在侧栏「自定义线路」添加，或在「地域」勾选地区后自动生成。
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dns-line-src {
  position: relative;
  width: 100%;
}
.dns-line-src-trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  text-align: left;
  cursor: pointer;
}
.dns-line-src-value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dns-line-src-caret {
  flex: none;
  transform: rotate(90deg);
  transition: transform 0.15s ease;
  opacity: 0.55;
}
.dns-line-src-caret.open {
  transform: rotate(-90deg);
}
.dns-line-src-panel {
  position: absolute;
  z-index: 40;
  top: calc(100% + 4px);
  left: 0;
  display: flex;
  min-height: 220px;
  max-height: 340px;
  background: var(--surface, #fff);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: 0 8px 28px rgba(15, 23, 42, 0.12);
  overflow: hidden;
}
.dns-line-src-col {
  display: flex;
  flex-direction: column;
  min-width: 132px;
  max-width: 180px;
  overflow: auto;
  border-right: 1px solid var(--border);
  padding: 6px 0;
}
.dns-line-src-col.wide {
  min-width: 160px;
  max-width: 220px;
}
.dns-line-src-col:last-child {
  border-right: none;
}
.dns-line-src-col.cats {
  background: var(--bg-2, #f7f8fa);
  min-width: 112px;
}
.dns-line-src-cat {
  appearance: none;
  border: 0;
  background: transparent;
  text-align: left;
  padding: 9px 14px;
  font-size: 13px;
  color: var(--text-2);
  cursor: pointer;
}
.dns-line-src-cat:hover {
  background: rgba(0, 0, 0, 0.04);
  color: var(--text);
}
.dns-line-src-cat.active {
  background: var(--surface, #fff);
  color: var(--brand, #1677ff);
  font-weight: 500;
}
.dns-line-src-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 12px;
  font-size: 13px;
  color: var(--text);
  background: transparent;
  border: 0;
  text-align: left;
  cursor: pointer;
}
.dns-line-src-item:hover,
.dns-line-src-item.active {
  background: var(--brand-bg, rgba(22, 119, 255, 0.08));
}
.dns-line-src-item.check {
  cursor: pointer;
}
.dns-line-src-item input {
  flex: none;
}
.dns-line-src-meta {
  margin-left: auto;
  font-size: 11px;
  color: var(--text-3);
}
.dns-line-src-col-title {
  padding: 6px 12px 4px;
  font-size: 11px;
  color: var(--text-3);
}
.dns-line-src-empty {
  margin: 12px;
  font-size: 12px;
  color: var(--text-3);
  line-height: 1.5;
}
.check {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
</style>
