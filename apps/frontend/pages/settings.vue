<script setup lang="ts">
const settings = useSettingsConsole()
provide('settingsConsole', settings)

const {
  openEmail,
  saveEmail,
  openProfile,
  openPassword,
  saveProfile,
  savePassword,
  auth,
  profileOpen,
  profileName,
  profileError,
  passwordOpen,
  currentPassword,
  newPassword,
  passwordError,
  saving,
  emailOpen,
  newEmail,
  emailPassword,
  emailError,
  section,
} = settings

const TABS = [
  { value: 'basic', label: '基本设置', to: '/settings' },
  { value: 'advanced', label: '高级设置', to: '/settings/advanced', admin: true },
] as const

const visibleTabs = computed(() =>
  TABS.filter((t) => !('admin' in t && t.admin) || auth.isAdmin),
)
</script>

<template>
  <div class="page settings-console">
    <div class="page-head">
      <div>
        <div class="page-title">系统设置</div>
        <div class="page-desc">账号信息、接口地址与运行状态</div>
      </div>
    </div>

    <div class="tabs">
      <NuxtLink
        v-for="t in visibleTabs"
        :key="t.value"
        :to="t.to"
        class="tab"
        :class="{ active: section === t.value }"
      >
        {{ t.label }}
      </NuxtLink>
    </div>

    <NuxtPage />


    <div class="alert alert-warning mt-16">
      <AppIcon name="shield" :size="16" class="icon" />
      <span>如遇访问异常，请联系系统管理员。</span>
    </div>

    <BaseModal
      v-model="profileOpen"
      title="编辑个人资料"
      confirm-text="保存"
      :loading="saving"
      @confirm="saveProfile"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">显示名称</label>
          <input v-model="profileName" class="input" />
        </div>
        <p v-if="profileError" class="text-danger" style="font-size: 13px">{{ profileError }}</p>
      </div>
    </BaseModal>

    <BaseModal
      v-model="passwordOpen"
      title="修改密码"
      confirm-text="修改"
      :loading="saving"
      @confirm="savePassword"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">当前密码</label>
          <input v-model="currentPassword" class="input" type="password" />
        </div>
        <div class="field">
          <label class="field-label">新密码</label>
          <input v-model="newPassword" class="input" type="password" placeholder="至少 8 位，含字母和数字" />
        </div>
        <p v-if="passwordError" class="text-danger" style="font-size: 13px">{{ passwordError }}</p>
      </div>
    </BaseModal>

    <BaseModal
      v-model="emailOpen"
      title="修改登录账号"
      confirm-text="修改"
      :loading="saving"
      @confirm="saveEmail"
    >
      <div class="form-col">
        <div class="field">
          <label class="field-label">当前账号</label>
          <input :value="auth.user?.email" class="input" disabled />
        </div>
        <div class="field">
          <label class="field-label">新账号</label>
          <input v-model="newEmail" class="input" type="text" autocomplete="off" placeholder="admin 或 you@example.com" />
          <span class="field-hint">可用管理员账号或邮箱；修改后其他已登录设备需重新登录。</span>
        </div>
        <div class="field">
          <label class="field-label">当前密码</label>
          <input v-model="emailPassword" class="input" type="password" autocomplete="current-password" />
        </div>
        <p v-if="emailError" class="text-danger" style="font-size: 13px">{{ emailError }}</p>
      </div>
    </BaseModal>
  </div>
</template>

<style>
.settings-console .tabs {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}
.settings-console .tab {
  border: 1px solid var(--border, #ddd);
  background: transparent;
  border-radius: 8px;
  padding: 6px 14px;
  cursor: pointer;
  font-size: 13px;
  text-decoration: none;
  color: inherit;
}
.settings-console .tab.active {
  background: var(--bg-muted, #f3f3f3);
  font-weight: 600;
}
.settings-console .profile {
  display: flex;
  align-items: center;
  gap: 14px;
}
.settings-console .profile-avatar {
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--brand-bg);
  color: var(--brand);
  font-size: 20px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  flex: none;
}
.settings-console .profile-name {
  font-size: 16px;
  font-weight: 600;
}
.settings-console .profile-mail {
  font-size: 13px;
  color: var(--text-3);
  margin-top: 2px;
}
.settings-console .kv-list {
  border-top: 1px solid var(--divider);
  padding-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.settings-console .kv {
  display: flex;
  gap: 16px;
  font-size: 13px;
}
.settings-console .kv-key {
  width: 84px;
  flex: none;
  color: var(--text-3);
}
.settings-console .kv-val {
  color: var(--text-2);
  word-break: break-all;
}
.settings-console .copy-row {
  display: flex;
  gap: 8px;
}
.settings-console .copy-row .input {
  flex: 1;
  font-size: 13px;
}
.settings-console .info-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px;
}
.settings-console .info-label {
  font-size: 12px;
  color: var(--text-3);
}
.settings-console .info-value {
  margin-top: 4px;
  font-size: 14px;
  color: var(--text);
}
.settings-console .update-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
}
.settings-console .update-metric {
  padding: 12px 14px;
  border: 1px solid var(--divider);
  border-radius: var(--radius);
  background: var(--bg);
}
.settings-console .update-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 16px;
  padding: 10px 12px;
  border: 1px solid var(--warning-border);
  border-radius: var(--radius);
  background: var(--warning-bg);
  color: var(--warning);
  font-size: 13px;
}
@media (max-width: 900px) {
.settings-console .info-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
.settings-console .update-grid {
    grid-template-columns: minmax(0, 1fr);
  }


}
@media (max-width: 640px) {
.settings-console .profile {
    flex-wrap: wrap;
  }
.settings-console .profile .flex-1 {
    min-width: 0;
  }
.settings-console .kv {
    flex-wrap: wrap;
    gap: 4px 12px;
  }
.settings-console .kv-key {
    width: auto;
    min-width: 72px;
  }
.settings-console .copy-row {
    flex-direction: column;
    align-items: stretch;
  }
.settings-console .copy-row .input {
    width: 100%;
    min-width: 0;
  }
.settings-console .copy-row .btn {
    width: 100%;
  }
.settings-console .flex.gap-8 {
    flex-wrap: wrap;
  }
.settings-console .info-grid {
    grid-template-columns: minmax(0, 1fr);
  }
.settings-console .info-value {
    word-break: break-word;
  }


}
.settings-console .card-body.is-disabled {
  opacity: 0.55;
}
.settings-console .card-body.is-disabled .btn:disabled {
  cursor: not-allowed;
}
</style>
