<script setup lang="ts">
import { siteLabel } from '~/utils/siteLabel'

const api = useApi()
const toast = useToast()
const id = inject<Ref<string>>('domainId')!
const domain = inject<Ref<any>>('domainRef')!
const reloadDomain = inject<() => Promise<void>>('reloadDomain')!

const submitting = ref(false)
const editError = ref('')

interface OriginForm {
  host: string
  port: number
  proto: string
  weight: number
  priority: number
  enabled: boolean
  sni?: string
  host_header?: string
}

const editForm = ref({
  origins: [] as OriginForm[],
  ssl_enabled: true,
})

const enabledOriginCount = computed(
  () => editForm.value.origins.filter((origin) => origin.enabled).length,
)

function hydrate() {
  const d = domain.value
  if (!d) return
  editError.value = ''
  const pool: OriginForm[] =
    Array.isArray(d.origins) && d.origins.length
      ? d.origins.map((origin: any) => ({
          host: origin.host,
          port: origin.port,
          proto: origin.proto,
          weight: origin.weight ?? 100,
          priority: origin.priority ?? 0,
          enabled: origin.enabled !== false,
          sni: origin.sni || '',
          host_header: origin.host_header || '',
        }))
      : [
          {
            host: d.origin_host,
            port: d.origin_port,
            proto: d.origin_proto,
            weight: 100,
            priority: 0,
            enabled: true,
            sni: '',
            host_header: '',
          },
        ]
  editForm.value = {
    origins: pool,
    ssl_enabled: !!d.ssl_enabled,
  }
}

function addOrigin() {
  editForm.value.origins.push({
    host: '',
    port: 443,
    proto: 'https',
    weight: 100,
    priority: 0,
    enabled: true,
    sni: '',
    host_header: '',
  })
}

function removeOrigin(index: number) {
  if (editForm.value.origins.length <= 1) return
  editForm.value.origins.splice(index, 1)
}

async function saveOrigin() {
  editError.value = ''
  const pool = editForm.value.origins
    .map((origin) => ({
      host: origin.host.trim(),
      port: Number(origin.port) || (origin.proto === 'https' ? 443 : 80),
      proto: origin.proto,
      weight: Math.max(1, Number(origin.weight) || 100),
      priority: Math.max(0, Number(origin.priority) || 0),
      enabled: origin.enabled,
      sni: origin.sni?.trim() || '',
      host_header: origin.host_header?.trim() || '',
    }))
    .filter((origin) => origin.host)
  if (!pool.length) {
    editError.value = '至少填写一个源站主机'
    return
  }
  if (!pool.some((origin) => origin.enabled)) {
    editError.value = '至少启用一个源站'
    return
  }
  const primary = pool.find((origin) => origin.enabled) ?? pool[0]
  if (!primary) {
    editError.value = '至少填写一个源站主机'
    return
  }
  submitting.value = true
  try {
    await api.put(`/api/v1/domains/${id.value}`, {
      origin_host: primary.host,
      origin_port: primary.port,
      origin_proto: primary.proto,
      ssl_enabled: editForm.value.ssl_enabled,
      origins: pool,
    })
    toast.success('源站池已保存')
    await reloadDomain()
    hydrate()
  } catch (e: any) {
    editError.value = e?.data?.error || '保存失败'
  } finally {
    submitting.value = false
  }
}

watch(domain, hydrate, { immediate: true, deep: true })
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">源站</div>
        <div class="card-desc">配置源站池、权重与 HTTPS 跳转</div>
      </div>
      <div class="card-extra">
        <button class="btn btn-primary btn-sm" type="button" :disabled="submitting" @click="saveOrigin">
          {{ submitting ? '保存中…' : '保存源站' }}
        </button>
      </div>
    </div>
    <div class="card-body">
<div class="origin-editor">
        <div v-if="domain" class="origin-context">
          <span class="origin-context-label">当前域名</span>
          <strong class="origin-context-name">{{ siteLabel(domain) }}</strong>
          <span class="origin-context-address">
            {{ domain.origin_proto }}://{{ domain.origin_host }}:{{ domain.origin_port }}
          </span>
        </div>

        <div class="origin-editor-head">
          <div class="origin-title-line">
            <span class="origin-editor-title">源站池</span>
            <span class="origin-count" :class="{ empty: !enabledOriginCount }">
              <span class="origin-count-dot" />
              {{ enabledOriginCount }}/{{ editForm.origins.length }} 已启用
            </span>
          </div>
          <button class="btn btn-default btn-sm origin-add-btn" type="button" @click="addOrigin">
            <AppIcon name="plus" :size="14" />
            添加源站
          </button>
        </div>

        <div class="origin-list">
          <div v-for="(origin, index) in editForm.origins" :key="index" class="origin-card"
            :class="{ 'is-disabled': !origin.enabled }">
            <div class="origin-card-head">
              <div class="origin-identity">
                <span class="origin-number">{{ String(index + 1).padStart(2, '0') }}</span>
                <div class="origin-name-wrap">
                  <div class="origin-name">{{ origin.host || '新源站' }}</div>
                  <div class="origin-meta">
                    <span>{{ origin.proto.toUpperCase() }}</span>
                    <span class="origin-meta-separator" />
                    <span>权重 {{ origin.weight || 100 }}</span>
                    <span>优先级 {{ origin.priority ?? 0 }}</span>
                  </div>
                </div>
              </div>
              <div class="origin-card-actions">
                <label class="origin-enable" :class="{ active: origin.enabled }">
                  <input v-model="origin.enabled" type="checkbox" />
                  <span class="origin-switch" aria-hidden="true" />
                  <span>{{ origin.enabled ? '已启用' : '已停用' }}</span>
                </label>
                <button class="origin-delete" type="button" aria-label="删除该源站" :disabled="editForm.origins.length <= 1"
                  @click="removeOrigin(index)">
                  <AppIcon name="trash" :size="14" />
                  <span>删除</span>
                </button>
              </div>
            </div>

            <div class="origin-card-body">
              <div class="origin-config">
                <label class="origin-field origin-proto-field">
                  <span>协议</span>
                  <select v-model="origin.proto" class="select">
                    <option value="https">HTTPS</option>
                    <option value="http">HTTP</option>
                  </select>
                </label>
                <label class="origin-field origin-host-field">
                  <span>源站主机 / IP</span>
                  <input v-model="origin.host" class="input" inputmode="url" autocapitalize="none" spellcheck="false"
                    placeholder="origin.example.com 或 203.0.113.10" />
                </label>
                <label class="origin-field origin-port-field">
                  <span>端口</span>
                  <input v-model.number="origin.port" class="input" type="number" min="1" max="65535" />
                </label>
              </div>

              <div class="origin-tuning">
                <label class="origin-field">
                  <span>权重</span>
                  <input v-model.number="origin.weight" class="input" type="number" min="1" placeholder="100" />
                </label>
                <label class="origin-field">
                  <span>优先级</span>
                  <input v-model.number="origin.priority" class="input" type="number" min="0" placeholder="0" />
                </label>
                <label class="origin-field">
                  <span>TLS SNI <em>可选</em></span>
                  <input v-model="origin.sni" class="input" inputmode="url" autocapitalize="none" spellcheck="false"
                    placeholder="证书域名" />
                </label>
                <label class="origin-field">
                  <span>回源 Host <em>可选</em></span>
                  <input v-model="origin.host_header" class="input" inputmode="url" autocapitalize="none"
                    spellcheck="false" placeholder="留空用访客访问的域名" />
                </label>
              </div>
            </div>
          </div>
        </div>

        <div class="origin-options">
          <span class="origin-option-icon">
            <AppIcon name="shield" :size="16" />
          </span>
          <div class="origin-option-copy">
            <label class="origin-enable origin-https-toggle" :class="{ active: editForm.ssl_enabled }">
              <input v-model="editForm.ssl_enabled" type="checkbox" />
              <span class="origin-switch" aria-hidden="true" />
              <span>启用 HTTPS 跳转</span>
            </label>
            <small>HTTP 请求 301/308 跳转到 HTTPS；源站为 IP + 域名证书时请填写 TLS SNI。回源 Host 默认是访客访问的域名，源站是对象存储等第三方域名时再填。</small>
          </div>
        </div>

        <div v-if="editError" class="alert alert-danger origin-error" role="alert">
          <AppIcon name="alert" :size="16" class="icon" />
          <span>{{ editError }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.origin-editor {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.origin-context {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px 9px;
  min-width: 0;
  padding: 7px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
}

.origin-context-label {
  padding: 2px 7px;
  border-radius: 5px;
  background: var(--brand-bg);
  color: var(--brand);
  font-size: 11px;
  font-weight: 500;
}

.origin-context-name {
  min-width: 0;
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.origin-context-address {
  min-width: 0;
  margin-left: auto;
  color: var(--text-3);
  font-family: var(--mono);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.origin-editor-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.origin-intro {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  min-width: 0;
}

.origin-intro-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  flex: none;
  border-radius: 10px;
  background: var(--brand-bg);
  color: var(--brand);
}

.origin-intro-copy {
  min-width: 0;
}

.origin-title-line {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  min-height: 22px;
}

.origin-editor-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.origin-count {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 20px;
  padding: 0 7px;
  border: 1px solid var(--success-border);
  border-radius: 999px;
  background: var(--success-bg);
  color: var(--success);
  font-size: 10px;
  line-height: 1;
  white-space: nowrap;
}

.origin-count-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
}

.origin-count.empty {
  border-color: var(--border);
  background: var(--bg);
  color: var(--text-3);
}

.origin-editor-sub {
  margin-top: 4px;
  color: var(--text-3);
  font-size: 12px;
  line-height: 1.6;
}

.origin-add-btn {
  flex: none;
}

.origin-list {
  display: flex;
  flex-direction: column;
  gap: 9px;
}

.origin-card {
  border: 1px solid var(--border);
  border-radius: 12px;
  background: var(--card);
  overflow: hidden;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.02);
  transition: border-color 0.15s ease, box-shadow 0.15s ease, opacity 0.15s ease;
}

.origin-card:hover {
  border-color: color-mix(in srgb, var(--brand) 35%, var(--border));
  box-shadow: 0 4px 14px rgba(15, 23, 42, 0.05);
}

.origin-card.is-disabled {
  background: color-mix(in srgb, var(--card) 86%, var(--bg));
}

.origin-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--divider);
  background: color-mix(in srgb, var(--brand) 2%, var(--card));
}

.origin-card.is-disabled .origin-card-head {
  background: var(--bg);
}

.origin-identity {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.origin-number {
  width: 24px;
  height: 24px;
  border: 1px solid color-mix(in srgb, var(--brand) 18%, transparent);
  border-radius: 7px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  background: var(--brand-bg);
  color: var(--brand);
  font-family: var(--mono);
  font-size: 10px;
  font-weight: 600;
}

.origin-name-wrap {
  min-width: 0;
}

.origin-name {
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.origin-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 2px;
  color: var(--text-3);
  font-size: 10px;
}

.origin-meta-separator {
  width: 1px;
  height: 9px;
  background: var(--border-strong);
}

.origin-card-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}

.origin-enable {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-height: 30px;
  padding: 0 9px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--card);
  color: var(--text-2);
  font-size: 12px;
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  transition: border-color 0.15s ease, background 0.15s ease, color 0.15s ease;
}

.origin-enable:hover {
  border-color: var(--border-strong);
}

.origin-enable.active {
  border-color: var(--success-border);
  background: var(--success-bg);
  color: var(--success);
}

.origin-enable input {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}

.origin-enable:focus-within {
  outline: 2px solid rgba(22, 119, 255, 0.28);
  outline-offset: 2px;
}

.origin-switch {
  position: relative;
  width: 28px;
  height: 16px;
  flex: none;
  border-radius: 999px;
  background: var(--text-4);
  transition: background 0.15s ease;
}

.origin-switch::after {
  content: "";
  position: absolute;
  top: 2px;
  left: 2px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.24);
  transition: transform 0.15s ease;
}

.origin-enable.active .origin-switch {
  background: var(--success);
}

.origin-enable.active .origin-switch::after {
  transform: translateX(12px);
}

.origin-delete {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  height: 30px;
  padding: 0 8px;
  border: none;
  border-radius: var(--radius);
  background: transparent;
  color: var(--text-3);
  font-size: 12px;
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.origin-delete:hover:not(:disabled) {
  background: var(--danger-bg);
  color: var(--danger);
}

.origin-delete:focus-visible {
  outline: 2px solid rgba(22, 119, 255, 0.28);
  outline-offset: 2px;
}

.origin-delete:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.origin-card-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
}

.origin-config {
  display: grid;
  grid-template-columns: 88px minmax(0, 1fr) 86px;
  gap: 8px;
}

.origin-tuning {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}

.origin-advanced {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.origin-subgroup {
  min-width: 0;
  padding: 12px;
  border: 1px solid var(--divider);
  border-radius: 9px;
  background: color-mix(in srgb, var(--bg) 72%, var(--card));
}

.origin-subgroup-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 10px;
}

.origin-subgroup-head>span {
  color: var(--text-2);
  font-size: 12px;
  font-weight: 600;
}

.origin-subgroup-head em {
  color: var(--text-3);
  font-style: normal;
  font-weight: 400;
}

.origin-subgroup-head small {
  color: var(--text-3);
  font-size: 10px;
  line-height: 1.4;
  text-align: right;
}

.origin-subgroup-fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.origin-field {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.origin-field>span {
  color: var(--text-2);
  font-size: 11px;
}

.origin-field>span em {
  color: var(--text-3);
  font-style: normal;
  font-weight: 400;
}

.origin-field .input,
.origin-field .select {
  height: 32px;
  width: 100%;
  min-width: 0;
}

.origin-options {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 9px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: color-mix(in srgb, var(--brand) 2%, var(--card));
}

.origin-option-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  flex: none;
  border-radius: 7px;
  background: var(--brand-bg);
  color: var(--brand);
}

.origin-option-copy {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 2px;
}

.origin-https-toggle {
  align-self: flex-start;
  min-height: 20px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--text);
  font-size: 12px;
  font-weight: 500;
}

.origin-https-toggle:hover {
  border-color: transparent;
}

.origin-https-toggle.active {
  background: transparent;
}

.origin-option-copy small {
  color: var(--text-3);
  font-size: 11px;
  line-height: 1.45;
}

.origin-help {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  color: var(--text-3);
  font-size: 12px;
  line-height: 1.55;
}

.origin-help .app-icon {
  margin-top: 2px;
  color: var(--text-4);
}

.origin-error {
  margin: 0;
}

.check-col {
  width: 38px;
}

.bulk-bar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px 12px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--brand-bg);
  color: var(--text-2);
  font-size: 13px;
}

.bulk-bar b {
  color: var(--brand);
}

@media (max-width: 720px) {
  .origin-editor {
    gap: 12px;
  }

  .origin-context-address {
    width: 100%;
    margin-left: 0;
    padding-left: 2px;
  }

  .origin-editor-head {
    flex-direction: column;
    gap: 12px;
  }

  .origin-add-btn {
    width: 100%;
    justify-content: center;
  }

  .origin-card {
    border-radius: 10px;
  }

  .origin-card-head {
    align-items: flex-start;
    gap: 10px;
  }

  .origin-card-actions {
    gap: 4px;
  }

  .origin-enable {
    min-height: 34px;
    padding: 0 8px;
  }

  .origin-delete {
    min-width: 34px;
    min-height: 34px;
    padding: 0 6px;
  }

  .origin-delete span {
    display: none;
  }

  .origin-card-body {
    gap: 8px;
    padding: 10px;
  }

  .origin-config {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }

  .origin-host-field {
    order: -1;
    grid-column: 1 / -1;
  }

  .origin-tuning {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .origin-field .input,
  .origin-field .select {
    height: 36px;
  }

  .origin-options {
    padding: 10px;
  }
}

@media (max-width: 520px) {
  .origin-intro-icon {
    width: 32px;
    height: 32px;
  }

  .origin-card-head {
    flex-wrap: wrap;
    padding: 12px;
  }

  .origin-card-actions {
    width: 100%;
    justify-content: space-between;
  }

  .origin-delete {
    min-width: auto;
    padding: 0 8px;
  }

  .origin-delete span {
    display: inline;
  }

  .origin-subgroup-head {
    align-items: flex-start;
    flex-direction: column;
    gap: 2px;
  }

  .origin-subgroup-head small {
    text-align: left;
  }

  .origin-tls-fields {
    grid-template-columns: minmax(0, 1fr);
  }

  .origin-options {
    gap: 9px;
  }

  .origin-option-icon {
    width: 28px;
    height: 28px;
  }
}

/* Status row for write-only secrets: badge + explanation + action. */
.secret-row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.secret-row .input {
  flex: 1;
  min-width: 0;
}

/* Manual key entry is an escape hatch, not the normal path. */
.advanced-secret {
  margin: 4px 0 12px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-subtle, transparent);
}

.advanced-secret summary {
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-2);
}

.advanced-secret[open] summary {
  margin-bottom: 4px;
}

/* Grouped settings: each section is a visual unit so a long drawer can be
   scanned instead of read top to bottom. */

</style>
