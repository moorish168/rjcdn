<script setup lang="ts">
const settings = inject('settingsConsole') as ReturnType<typeof useSettingsConsole>
const {
  openEmail,
  openProfile,
  openPassword,
  copy,
  checkHealth,
  auth,
  brand,
  effectiveApiBase,
  checking,
  healthStatus,
  controlVersion,
  initial,
} = settings
</script>

<template>
  <div class="settings-basic">
    <div class="grid grid-2">
      <div class="card">
        <div class="card-head">
          <div class="card-title">个人资料</div>
        </div>
        <div class="card-body">
          <div class="profile">
            <div class="profile-avatar">{{ initial }}</div>
            <div class="flex-1">
              <div class="profile-name">{{ auth.user?.name || '未命名用户' }}</div>
              <div class="profile-mail">{{ auth.user?.email }}</div>
            </div>
            <StatusTag :tone="auth.isAdmin ? 'info' : 'neutral'">
              {{ auth.isAdmin ? '管理员' : '普通用户' }}
            </StatusTag>
          </div>
          <div class="kv-list mt-16">
            <div class="kv">
              <span class="kv-key">用户 ID</span>
              <span class="kv-val mono">{{ auth.user?.id || '—' }}</span>
            </div>
            <div class="kv">
              <span class="kv-key">登录方式</span>
              <span class="kv-val">账号 + 密码（JWT）</span>
            </div>
            <div class="kv">
              <span class="kv-key">权限范围</span>
              <span class="kv-val">
                {{ auth.isAdmin ? '全部项目与域名、节点与审计' : '所属项目内的域名与配置' }}
              </span>
            </div>
          </div>
          <div class="flex gap-8 mt-16">
            <button class="btn btn-default btn-sm" type="button" @click="openProfile">
              编辑资料
            </button>
            <button class="btn btn-default btn-sm" type="button" @click="openPassword">
              修改密码
            </button>
            <button class="btn btn-default btn-sm" type="button" @click="openEmail">
              修改登录账号
            </button>
          </div>
          <p class="text-muted mt-16" style="font-size: 12px">
            账号由系统管理员统一管理。
          </p>
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <div class="card-title">接口配置</div>
        </div>
        <div class="card-body">
          <div class="field">
            <label class="field-label">控制端 API 地址</label>
            <div class="copy-row">
              <input class="input mono" :value="effectiveApiBase" readonly />
              <button class="btn btn-default" type="button" @click="copy(effectiveApiBase, 'API 地址')">
                <AppIcon name="copy" :size="15" />
                复制
              </button>
            </div>
            <span class="field-hint">
              未单独配置时使用当前站点同源地址，由反向代理转发到控制面。
            </span>
          </div>

          <div class="field mt-16">
            <label class="field-label">控制面连通性</label>
            <div class="flex-center gap-12">
              <button class="btn btn-default" type="button" :disabled="checking" @click="checkHealth">
                <AppIcon name="activity" :size="15" />
                {{ checking ? '检测中…' : '检测' }}
              </button>
              <StatusTag v-if="healthStatus" :tone="healthStatus.ok ? 'success' : 'danger'" dot>
                {{ healthStatus.ok ? '连接正常' : '连接失败' }}
              </StatusTag>
              <span v-if="healthStatus?.at" class="text-muted" style="font-size: 12px">
                {{ healthStatus.at }}
              </span>
            </div>
            <span v-if="healthStatus && !healthStatus.ok" class="field-hint text-danger">
              {{ healthStatus.message }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card-head">
        <div class="card-title">系统信息</div>
      </div>
      <div class="card-body">
        <div class="info-grid">
          <div class="info-item">
            <div class="info-label">系统名称</div>
            <div class="info-value">{{ brand.systemName }}</div>
          </div>
          <div class="info-item">
            <div class="info-label">控制面版本</div>
            <div class="info-value mono">{{ controlVersion || '—' }}</div>
          </div>
          <div class="info-item">
            <div class="info-label">当前角色</div>
            <div class="info-value">{{ auth.isAdmin ? '系统管理员' : '普通用户' }}</div>
          </div>
          <div class="info-item">
            <div class="info-label">会话策略</div>
            <div class="info-value">密码修改后旧会话立即失效</div>
          </div>
          <div class="info-item">
            <div class="info-label">接入方式</div>
            <div class="info-value">同源 API + 边缘节点</div>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>
