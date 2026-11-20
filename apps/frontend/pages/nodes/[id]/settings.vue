<script setup lang="ts">
import { friendlyErrorMessage } from '~/utils/friendlyError'
import type { useNodeDetail } from '~/composables/useNodeDetail'
import type { GeoRegionDto } from '~/composables/useDnsConsole'
import {
  countriesFromDnsLine,
  resolveDnsLineId,
} from '~/composables/ensureDnsLine'

const api = useApi()
const toast = useToast()
const geo = useGeoCatalog()
const ctx = inject('nodeDetail') as ReturnType<typeof useNodeDetail>
const { detail, load } = ctx

const saving = ref(false)
const formError = ref('')
const geoRegions = ref<GeoRegionDto[]>([])
const storagePolicies = ref<any[]>([])
const dnsLineCountries = ref<string[]>([])
const dnsLineId = ref('')
const form = ref({
  name: '',
  region: '',
  agent_url: '',
  public_ip: '',
  country: 'CN',
  province: '',
  city: '',
  isp: '',
  weight: 100,
  cache_storage_policy_id: '',
})

async function hydrate() {
  const node = detail.value?.node
  if (!node) return
  formError.value = ''
  if (!geoRegions.value.length) {
    void geo.load()
    geoRegions.value = (await api.get<GeoRegionDto[]>('/api/v1/geo-regions').catch(() => [])) || []
  }
  if (!storagePolicies.value.length) {
    storagePolicies.value =
      (await api.get<any[]>('/api/v1/cache-storage-policies').catch(() => [])) || []
  }
  form.value = {
    name: node.name,
    region: node.region || '',
    agent_url: node.agent_url || '',
    public_ip: node.public_ip || '',
    country: node.country || 'CN',
    province: node.province || '',
    city: node.city || '',
    isp: node.isp || '',
    weight: node.weight ?? 100,
    cache_storage_policy_id: node.cache_storage_policy_id || '',
  }
  const line = geoRegions.value.find((r) => r.id === (node.region_id || ''))
  if (line && !line.is_default) {
    const countries = countriesFromDnsLine(line, geo.continentCountries)
    // ISP / province lines keep explicit id; area-only lines reopen as「地域」勾选.
    if (line.isp?.trim() || line.province?.trim() || line.city?.trim()) {
      dnsLineId.value = line.id
      dnsLineCountries.value = countries
    } else {
      dnsLineId.value = ''
      dnsLineCountries.value = countries
    }
  } else {
    dnsLineId.value = ''
    dnsLineCountries.value = []
  }
}

async function submitForm() {
  formError.value = ''
  if (!detail.value?.node?.id) return
  saving.value = true
  try {
    const regionId = await resolveDnsLineId(api, {
      lineId: dnsLineId.value,
      countries: dnsLineCountries.value,
      note: '由节点设置自动创建',
    })
    if (regionId) {
      geoRegions.value = (await api.get<GeoRegionDto[]>('/api/v1/geo-regions').catch(() => geoRegions.value)) || geoRegions.value
    }
    if (dnsLineCountries.value.length === 1) {
      form.value.country = dnsLineCountries.value[0]
    }
    await api.put(`/api/v1/nodes/${detail.value.node.id}`, {
      region: form.value.region.trim(),
      public_ip: form.value.public_ip.trim(),
      country: form.value.country.trim().toUpperCase() || 'CN',
      region_id: regionId || '',
    })
    await api.put(`/api/v1/nodes/${detail.value.node.id}/cache-storage-policy`, {
      cache_storage_policy_id: form.value.cache_storage_policy_id || '',
    })
    toast.success('节点信息已更新')
    await load()
    await hydrate()
  } catch (e: any) {
    const message = friendlyErrorMessage(e?.data?.error, '保存失败，请稍后重试')
    formError.value = message
    toast.error(message)
  } finally {
    saving.value = false
  }
}

watch(detail, hydrate, { immediate: true, deep: true })
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">基本信息</div>
        <div class="card-desc">公网 IP 与 DNS 线路决定主域如何调度；节点地址请在「硬件画像」维护</div>
      </div>
      <div class="card-extra">
        <button class="btn btn-primary btn-sm" type="button" :disabled="saving" @click="submitForm">
          {{ saving ? '保存中…' : '保存' }}
        </button>
      </div>
    </div>
    <div class="card-body">
      <div class="form-grid">
        <div class="field">
          <label class="field-label">节点名称</label>
          <input :value="form.name" class="input" disabled />
        </div>
        <div class="field">
          <label class="field-label">公网 IP</label>
          <input v-model="form.public_ip" class="input" placeholder="写入 CDN 主域 A/AAAA 的 IP" />
        </div>
        <div class="field span-2">
          <label class="field-label">DNS 调度线路</label>
          <span class="field-hint">
            与添加 A 记录相同。保存后同步到 CDN 主域；已接入节点改这里即可换线路。
          </span>
          <DnsLineSourcePicker
            v-model:countries="dnsLineCountries"
            v-model:line-id="dnsLineId"
            :lines="geoRegions"
          />
        </div>
        <div class="field">
          <label class="field-label">缓存策略</label>
          <select v-model="form.cache_storage_policy_id" class="select">
            <option value="">未绑定</option>
            <option v-for="p in storagePolicies" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
          <span class="field-hint">磁盘目录与容量；在「节点 → 缓存策略」维护。</span>
        </div>
        <div class="field span-2">
          <label class="field-label">节点地址</label>
          <input :value="form.agent_url" class="input mono" readonly />
          <span class="field-hint">请到「设置 → 硬件画像」维护。</span>
        </div>
      </div>
      <p v-if="formError" class="text-danger" style="font-size: 13px; margin-top: 10px">{{ formError }}</p>
    </div>
  </div>
</template>
