<script setup lang="ts">
import type { useNodeDetail } from '~/composables/useNodeDetail'

const ctx = inject('nodeDetail') as ReturnType<typeof useNodeDetail>
const { detail, remove } = ctx
const busy = ref(false)

async function onDelete() {
  busy.value = true
  try {
    await remove()
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="card">
    <div class="card-head">
      <div>
        <div class="card-title">删除节点</div>
        <div class="card-desc">危险操作：吊销令牌并停止下发刷新 / 证书</div>
      </div>
    </div>
    <div class="card-body">
      <div class="alert alert-danger" role="alert">
        <AppIcon name="alert" :size="16" class="icon" />
        <span>
          删除 <strong>{{ detail?.node?.name || '该节点' }}</strong> 后会吊销其令牌。
          若进程仍在运行，下次心跳可能自动重新注册。
        </span>
      </div>
      <button class="btn btn-danger" type="button" :disabled="busy || !detail" @click="onDelete">
        {{ busy ? '删除中…' : '确认删除此节点' }}
      </button>
    </div>
  </div>
</template>
