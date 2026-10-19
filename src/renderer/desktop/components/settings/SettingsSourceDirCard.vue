<template>
  <SettingsCard :title="t.settings.sourceDirTitle" :icon="FolderOpened">
    <p class="section-desc">
      {{ t.settings.sourceDirDesc }}
    </p>

    <div class="source-dir-row">
      <input
        type="text"
        class="path-input"
        :placeholder="t.settings.sourceDirPlaceholder"
        :value="sourceDir"
        readonly
      >
      <button
        v-if="hasTauri"
        type="button"
        class="browse-btn"
        @click="browseSourceDir"
      >
        <FolderOpened />
        <span>{{ t.settings.sourceDirBrowse }}</span>
      </button>
      <button
        v-if="hasTauri && sourceDir"
        type="button"
        class="browse-btn"
        @click="clearSourceDir"
      >
        <span>{{ t.settings.sourceDirClear }}</span>
      </button>
    </div>

    <div v-if="validating" class="source-dir-hint muted">{{ t.settings.sourceDirValidating }}</div>
    <div v-else-if="status && status.valid" class="source-dir-hint ok">
      <span>✓ {{ t.settings.sourceDirValid }}</span>
      <span v-if="status.branch" class="source-branch">（{{ status.branch }}）</span>
      <span class="source-via">
        · {{ status.source === 'detected' ? t.settings.sourceDirViaDetected : t.settings.sourceDirViaSetting }}
      </span>
    </div>
    <div v-else-if="status && !status.valid" class="source-dir-hint bad">
      <span>✗ {{ status.reason }}</span>
    </div>
  </SettingsCard>
</template>

<script setup>
import { onMounted, ref, watch } from 'vue'
import { FolderOpened } from '@element-plus/icons-vue'
import { useI18n } from '../../i18n/index.js'
import SettingsCard from './SettingsCard.vue'

const props = defineProps({
  hasTauri: {
    type: Boolean,
    default: false,
  },
})

const { t } = useI18n()

const sourceDir = ref('')
const status = ref(null)
const validating = ref(false)

async function invokeValidate(path) {
  validating.value = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const result = await invoke('validate_sunshine_source_dir', { path: path || null })
    status.value = result
    sourceDir.value = result.path || ''
  } catch (error) {
    status.value = { valid: false, reason: String(error?.message ?? error) }
  } finally {
    validating.value = false
  }
}

async function browseSourceDir() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const dir = await open({ directory: true, multiple: false })
    if (!dir) return
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('set_sunshine_source_dir', { path: dir })
    await invokeValidate(dir)
  } catch (error) {
    status.value = { valid: false, reason: String(error?.message ?? error) }
  }
}

async function clearSourceDir() {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('set_sunshine_source_dir', { path: null })
    await invokeValidate(null)
  } catch (error) {
    status.value = { valid: false, reason: String(error?.message ?? error) }
  }
}

onMounted(() => {
  if (props.hasTauri) invokeValidate(null)
})

watch(
  () => props.hasTauri,
  (value) => {
    if (value && !status.value) invokeValidate(null)
  }
)
</script>

<style lang="less" scoped>
.section-desc {
  font-size: 14px;
  color: rgba(var(--fd-text-primary-rgb, 255, 255, 255), 0.5);
  margin: 0 0 16px 0;
  line-height: 1.5;
}

.source-dir-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}

.path-input {
  flex: 1;
  min-width: 0;
  padding: 8px 12px;
  border: 1px solid rgba(var(--fd-accent-rgb, 0, 255, 245), 0.2);
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.2);
  color: var(--fd-text-primary, #fff);
  font-size: 13px;
  font-family: 'Consolas', 'Monaco', monospace;

  &::placeholder {
    color: rgba(var(--fd-text-primary-rgb, 255, 255, 255), 0.25);
  }

  &:focus {
    outline: none;
    border-color: var(--fd-accent, #00fff5);
  }
}

.browse-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 12px;
  border: 1px solid rgba(var(--fd-accent-rgb, 0, 255, 245), 0.2);
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.2);
  color: var(--fd-text-primary, #fff);
  font-size: 13px;
  white-space: nowrap;
  cursor: pointer;
  transition: all 0.2s;
  flex-shrink: 0;

  &:hover {
    background: rgba(var(--fd-accent-rgb, 0, 255, 245), 0.1);
    border-color: var(--fd-accent, #00fff5);
  }
}

.source-dir-hint {
  margin-top: 10px;
  font-size: 13px;
  line-height: 1.5;

  &.muted {
    color: rgba(var(--fd-text-primary-rgb, 255, 255, 255), 0.4);
  }

  &.ok {
    color: #67c23a;
  }

  &.bad {
    color: #f56c6c;
    word-break: break-all;
  }

  .source-branch,
  .source-via {
    color: rgba(var(--fd-text-primary-rgb, 255, 255, 255), 0.5);
  }
}
</style>
