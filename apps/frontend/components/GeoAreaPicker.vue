<script setup lang="ts">
/**
 * 洲 → 国家/地区 multi-select. The model is the flat list of selected ISO
 * country codes; ticking a continent selects every country in it.
 */
import type { GeoCountry } from '~/composables/useGeoCatalog'

const props = defineProps<{ modelValue: string[] }>()
const emit = defineEmits<{ 'update:modelValue': [value: string[]] }>()

const geo = useGeoCatalog()
const keyword = ref('')
const opened = ref<Set<string>>(new Set())

onMounted(() => {
  void geo.load()
})

const selected = computed(() => new Set(props.modelValue))
const query = computed(() => keyword.value.trim().toLowerCase())

function matches(co: GeoCountry): boolean {
  const q = query.value
  if (!q) return true
  return (
    co.name_zh.toLowerCase().includes(q) ||
    co.name_en.toLowerCase().includes(q) ||
    co.code.toLowerCase() === q
  )
}

const groups = computed(() =>
  geo.catalog.value
    .map((cont) => ({
      code: cont.code,
      name: cont.name_zh,
      total: cont.countries.length,
      picked: cont.countries.filter((co) => selected.value.has(co.code)).length,
      visible: cont.countries.filter(matches),
    }))
    .filter((g) => !query.value || g.visible.length),
)

function isOpen(code: string): boolean {
  return !!query.value || opened.value.has(code)
}

function toggleOpen(code: string) {
  const next = new Set(opened.value)
  if (next.has(code)) next.delete(code)
  else next.add(code)
  opened.value = next
}

function emitSet(set: Set<string>) {
  emit('update:modelValue', [...set].sort())
}

function setContinent(code: string, checked: boolean) {
  const next = new Set(props.modelValue)
  for (const co of geo.continentCountries(code)) {
    if (checked) next.add(co)
    else next.delete(co)
  }
  emitSet(next)
}

function setCountry(code: string, checked: boolean) {
  const next = new Set(props.modelValue)
  if (checked) next.add(code)
  else next.delete(code)
  emitSet(next)
}

const summary = computed(() => {
  const { continents, countries } = geo.collapse(props.modelValue)
  return geo.areaLabel(continents, countries, 8)
})
</script>

<template>
  <div class="geo-picker">
    <div class="geo-picker-bar">
      <input
        v-model="keyword"
        class="input geo-picker-search"
        type="search"
        placeholder="搜索国家/地区，如 美国、Japan 或 JP"
      />
      <span class="geo-picker-total">已选 {{ modelValue.length }} 个</span>
      <button
        v-if="modelValue.length"
        class="btn btn-text btn-sm"
        type="button"
        @click="emit('update:modelValue', [])"
      >
        清空
      </button>
    </div>

    <div v-if="!geo.catalog.value.length" class="geo-picker-empty">正在加载地区目录…</div>
    <div v-else-if="!groups.length" class="geo-picker-empty">没有匹配「{{ keyword.trim() }}」的国家/地区</div>
    <div v-else class="geo-picker-tree">
      <section v-for="g in groups" :key="g.code" class="geo-group">
        <div class="geo-group-head">
          <button
            class="geo-group-toggle"
            type="button"
            :aria-expanded="isOpen(g.code)"
            :aria-label="`${isOpen(g.code) ? '收起' : '展开'}${g.name}`"
            @click="toggleOpen(g.code)"
          >
            <AppIcon name="chevron-right" :size="13" class="geo-chevron" :class="{ open: isOpen(g.code) }" />
          </button>
          <label class="check-row geo-group-check">
            <input
              type="checkbox"
              :checked="g.total > 0 && g.picked === g.total"
              :indeterminate.prop="g.picked > 0 && g.picked < g.total"
              @change="setContinent(g.code, ($event.target as HTMLInputElement).checked)"
            />
            <span>{{ g.name }}</span>
          </label>
          <span class="geo-group-count" :class="{ on: g.picked > 0 }">{{ g.picked }}/{{ g.total }}</span>
        </div>
        <div v-if="isOpen(g.code)" class="geo-group-body">
          <label v-for="co in g.visible" :key="co.code" class="check-row geo-country" :title="co.name_en">
            <input
              type="checkbox"
              :checked="selected.has(co.code)"
              @change="setCountry(co.code, ($event.target as HTMLInputElement).checked)"
            />
            <span class="geo-country-name">{{ co.name_zh }}</span>
            <span class="geo-country-code">{{ co.code }}</span>
          </label>
        </div>
      </section>
    </div>

    <div v-if="modelValue.length" class="geo-picker-summary">已选：{{ summary }}</div>
  </div>
</template>

<style scoped>
.geo-picker {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}
.geo-picker-bar {
  display: flex;
  align-items: center;
  gap: 10px;
}
.geo-picker-search {
  flex: 1;
  min-width: 0;
}
.geo-picker-total {
  flex: none;
  font-size: 12px;
  color: var(--text-3);
}
.geo-picker-empty {
  padding: 14px 12px;
  font-size: 12.5px;
  color: var(--text-3);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg);
}
.geo-picker-tree {
  max-height: 340px;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--card);
}
.geo-group + .geo-group {
  border-top: 1px solid var(--divider);
}
.geo-group-head {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px 6px 6px;
  background: var(--card);
}
.geo-group-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
}
.geo-group-toggle:hover {
  background: var(--bg);
  color: var(--text);
}
.geo-chevron {
  transition: transform 0.15s ease;
}
.geo-chevron.open {
  transform: rotate(90deg);
}
.geo-group-check {
  flex: 1;
  width: auto;
  min-width: 0;
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
}
.geo-group-count {
  flex: none;
  font-size: 12px;
  color: var(--text-4);
  font-variant-numeric: tabular-nums;
}
.geo-group-count.on {
  color: var(--brand);
}
.geo-group-body {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 2px 12px;
  padding: 2px 12px 10px 36px;
}
.geo-country {
  width: 100%;
  min-width: 0;
  padding: 3px 0;
  font-size: 12.5px;
  color: var(--text-2);
}
.geo-country-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.geo-country-code {
  flex: none;
  margin-left: auto;
  font-family: var(--mono);
  font-size: 11px;
  color: var(--text-4);
}
.geo-picker-summary {
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-2);
}
@media (max-width: 640px) {
  .geo-group-body {
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    padding-left: 12px;
  }
}
</style>
