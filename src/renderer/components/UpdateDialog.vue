<template>
  <el-dialog
    v-model="visible"
    :title="dialogTitle"
    width="min(720px, 92vw)"
    :close-on-click-modal="isLatest && !isSourceRunning && !isInstalling"
    :close-on-press-escape="isLatest || (!isInstalling && !isDownloading && !isSourceRunning)"
    :show-close="!isInstalling && !isDownloading && !isSourceRunning"
  >
    <div class="update-dialog-content">

      <!-- 本地源码模式入口：配置有效目录后自动重新检查并切换为源码模式 -->
      <div v-if="showSourcePicker" class="source-dir-panel">
        <div class="source-dir-header" @click="sourceSectionOpen = !sourceSectionOpen">
          <el-icon class="source-dir-icon"><FolderOpened /></el-icon>
          <span class="source-dir-title">{{ t.updateDialog.sourceModeHeader }}</span>
          <el-icon class="source-dir-arrow" :class="{ open: sourceSectionOpen }"><ArrowDown /></el-icon>
        </div>
        <div v-show="sourceSectionOpen" class="source-dir-body">
          <p class="source-dir-desc">{{ t.settings.sourceDirDesc }}</p>
          <div class="source-dir-row">
            <input
              type="text"
              class="source-dir-input"
              :placeholder="t.settings.sourceDirPlaceholder"
              :value="sourceDirState.path || ''"
              readonly
            >
            <el-button size="small" :loading="sourceDirBusy" @click="browseSourceDir">
              {{ t.settings.sourceDirBrowse }}
            </el-button>
            <el-button
              v-if="sourceDirState.path"
              size="small"
              :disabled="sourceDirBusy"
              @click="clearSourceDir"
            >{{ t.settings.sourceDirClear }}</el-button>
          </div>
          <p v-if="sourceRechecking" class="source-dir-hint muted">{{ t.updateDialog.sourceModeRechecking }}</p>
          <p v-else-if="sourceDirBusy" class="source-dir-hint muted">{{ t.settings.sourceDirValidating }}</p>
          <p v-else-if="sourceDirState.valid" class="source-dir-hint ok">
            <span>✓ {{ t.settings.sourceDirValid }}</span>
            <span v-if="sourceDirState.branch" class="source-dir-meta">（{{ sourceDirState.branch }}）</span>
            <span class="source-dir-meta">
              · {{ sourceDirState.source === 'detected' ? t.settings.sourceDirViaDetected : t.settings.sourceDirViaSetting }}
            </span>
          </p>
          <p v-else-if="sourceDirState.path" class="source-dir-hint bad">✗ {{ sourceDirState.reason }}</p>
        </div>
      </div>

      <!-- 更新说明 -->
      <div v-if="updateInfo?.release_notes" class="release-notes">
        <div class="notes-content" v-html="parsedReleaseNotes"></div>
      </div>

      <!-- 源码模式：同步 + 本地构建进度 -->
      <div v-if="isSourceBuild && (isSourceRunning || sourceLogLines.length)" class="source-progress">
        <div class="download-status">
          <span v-if="isSourceRunning && !sourceError" class="download-spinner"></span>
          <span v-else-if="!sourceError" class="download-complete-mark">✓</span>
          <p class="progress-text">{{ sourceStatusText }}</p>
        </div>

        <ol class="install-steps">
          <li
            v-for="(step, index) in sourceSteps"
            :key="step.key"
            :class="getSourceStepStatus(index)"
          >
            <span class="step-dot"></span>
            <span class="step-label">{{ step.label }}</span>
          </li>
        </ol>

        <pre v-if="sourceLogTail" class="source-log">{{ sourceLogTail }}</pre>
        <p v-if="sourceError" class="source-error">{{ sourceError }}</p>
      </div>

      <!-- 下载进度 (仅新版本、线上安装包模式) -->
      <div v-if="!isLatest && !isSourceBuild && isDownloading" class="download-progress">
        <div class="download-status">
          <span v-if="downloadPhase !== 'complete'" class="download-spinner"></span>
          <span v-else class="download-complete-mark">✓</span>
          <p class="progress-text">{{ downloadStatusText }}</p>
          <span
            v-if="downloadPhase === 'downloading' && downloadSource"
            class="download-source-chip"
          >
            {{ downloadSource }}
          </span>
        </div>
        <el-progress
          v-if="downloadProgress > 0"
          :percentage="downloadProgress"
          :stroke-width="8"
          :show-text="false"
          :status="downloadPhase === 'complete' ? 'success' : undefined"
        />
      </div>

      <!-- 安装包已就绪 (下载完成/本地构建完成/上次已下载) -->
      <div v-if="(!isLatest || isSourceBuild) && installReady" class="install-ready-panel">
        <p class="install-note">{{ isSourceBuild ? t.updateDialog.sourceInstallReadyHint : t.updateDialog.installReadyHint }}</p>
      </div>

      <!-- 安装提示 (仅新版本) -->
      <div v-if="isInstalling" class="install-progress-panel">
        <div class="install-status">
          <span class="install-spinner" :class="{ failed: installStage === 'failed' }"></span>
          <div>
            <p class="install-current">{{ currentInstallMessage }}</p>
            <p v-if="installDetail" class="install-detail">{{ installDetail }}</p>
          </div>
        </div>

        <ol class="install-steps">
          <li
            v-for="(step, index) in installSteps"
            :key="step.key"
            :class="getInstallStepStatus(index)"
          >
            <span class="step-dot"></span>
            <span class="step-label">{{ step.label }}</span>
          </li>
        </ol>

        <p class="install-note">{{ t.updateDialog.installDoNotRepeat }}</p>
        <p v-if="installWaitHint" class="install-wait-hint">{{ installWaitHint }}</p>
      </div>
    </div>

    <template #footer>
      <div class="dialog-footer">
        <template v-if="isLatest">
          <el-button @click="handleOpenBrowser">{{ t.updateDialog.viewInBrowser }}</el-button>
          <el-button
            v-if="isSourceBuild && isSourceRunning"
            type="primary"
            disabled
          >{{ t.updateDialog.sourceSyncBuilding }}</el-button>
          <el-button
            v-else-if="isSourceBuild && !isInstalling"
            type="primary"
            :loading="isSourceStarting"
            @click="handleStartSourceUpdate"
          >
            <el-icon><Download /></el-icon>
            {{ sourceError ? t.updateDialog.sourceRetryBuild : t.updateDialog.sourceRebuild }}
          </el-button>
          <el-button
            v-if="!isSourceRunning && !isInstalling"
            @click="handleCancel"
          >{{ t.updateDialog.close }}</el-button>
        </template>
        <template v-else>
          <el-button @click="handleOpenBrowser">{{ t.updateDialog.openInBrowser }}</el-button>

          <el-button
            v-if="isSourceBuild && !installReady && !isSourceRunning && !isInstalling"
            @click="handleCancel"
          >{{ t.updateDialog.remindLater }}</el-button>
          <el-button
            v-if="isSourceBuild && isSourceRunning && !installReady"
            type="primary"
            disabled
          >{{ t.updateDialog.sourceSyncBuilding }}</el-button>
          <el-button
            v-if="isSourceBuild && !installReady && !isSourceRunning && !isInstalling"
            type="primary"
            :loading="isSourceStarting"
            @click="handleStartSourceUpdate"
          >
            <el-icon><Download /></el-icon>
            {{ sourceError ? t.updateDialog.sourceRetryBuild : t.updateDialog.sourceSyncBuild }}
          </el-button>

          <el-button
            v-if="!isSourceBuild && showDownloadButtons"
            @click="handleSkipVersion"
          >{{ t.updateDialog.skipVersion }}</el-button>
          <el-button
            v-if="!isSourceBuild && showDownloadButtons"
            @click="handleCancel"
          >{{ t.updateDialog.remindLater }}</el-button>
          <el-button
            v-if="!isSourceBuild && showDownloadButtons"
            type="primary"
            :loading="isDownloading"
            @click="handleDownload"
          >
            <el-icon><Download /></el-icon>
            {{ t.updateDialog.downloadAndInstall }}
          </el-button>
          <el-button
            v-if="!isSourceBuild && isDownloading"
            type="primary"
            :loading="downloadPhase !== 'complete'"
            disabled
          >
            {{
              downloadPhase === 'complete'
                ? t.updateDialog.downloadComplete
                : t.updateDialog.downloading
            }}
          </el-button>

          <el-button
            v-if="installReady && !isInstalling"
            @click="handleCancel"
          >{{ t.updateDialog.remindLater }}</el-button>
          <el-button
            v-if="installReady && !isInstalling"
            type="primary"
            @click="handleInstall(downloadedFilePath, isSourceBuild ? 'nsis' : null)"
          >{{ t.updateDialog.installNow }}</el-button>
          <el-button
            v-if="!isSourceBuild && isInstalling"
            type="primary"
            disabled
          >{{ t.updateDialog.installing }}</el-button>
        </template>
      </div>
    </template>
  </el-dialog>
</template>

<script setup>
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Download, FolderOpened, ArrowDown } from '@element-plus/icons-vue'
import { useI18n } from '../desktop/i18n/index.js'
import { renderReleaseNotes } from '../utils/releaseNotes.js'

const { t } = useI18n()

const DOWNLOAD_PHASE_MESSAGE_KEYS = Object.freeze({
  connecting: 'downloadConnecting',
  retrying: 'downloadRetrying',
  verifying: 'downloadVerifying',
  complete: 'downloadComplete',
})

const DOWNLOAD_ERROR_MESSAGE_KEYS = Object.freeze({
  setup_failed: 'downloadSetupFailed',
  file_preparation_failed: 'downloadFilePreparationFailed',
  file_finalization_failed: 'downloadFileFinalizationFailed',
  sources_exhausted: 'downloadSourcesFailed',
})

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  updateInfo: { type: Object, default: null },
  currentVersion: { type: String, default: '0.0.0' },
})

const emit = defineEmits(['update:modelValue', 'close', 'skip-version', 'update-info'])

const visible = computed({
  get: () => props.modelValue,
  set: (val) => emit('update:modelValue', val),
})

const isDownloading = ref(false)
const downloadProgress = ref(0)
const downloadPhase = ref('idle')
const downloadSource = ref('')
const downloadedFilePath = ref('')
const isInstalling = ref(false)
const installStage = ref('idle')
const installDetail = ref('')
const installWaitHint = ref('')
const installLastEventAt = ref(0)
const installFailedRank = ref(null)
let installProgressUnlisten = null
let installWaitTimer = null

// 源码模式：fork 同步 + 本地 ninja/cpack 构建
const isSourceStarting = ref(false)
const isSourceRunning = ref(false)
const sourceStage = ref('idle')
const sourceStatusMessage = ref('')
const sourceError = ref('')
const sourceLogLines = ref([])
let sourceProgressUnlisten = null
const SOURCE_LOG_MAX = 300

const isLatest = computed(() => !!props.updateInfo?.is_latest)

const isSourceBuild = computed(() => props.updateInfo?.kind === 'source_build')

const dialogTitle = computed(() => {
  const ver = props.updateInfo?.version || ''
  if (isLatest.value) {
    return t.value.updateDialog.titleLatest.replace('{version}', ver)
  }
  return t.value.updateDialog.titleNew.replace('{version}', ver).replace('{current}', props.currentVersion)
})

const parsedReleaseNotes = computed(() =>
  renderReleaseNotes(props.updateInfo?.release_notes)
)

const showDownloadButtons = computed(
  () => !isDownloading.value && !isInstalling.value && downloadProgress.value === 0 && !downloadedFilePath.value
)

// 安装包已就绪：本次下载完成或上次会话已完整下载，可直接安装
const installReady = computed(
  () => !!downloadedFilePath.value
    && !isDownloading.value
    && !isInstalling.value
)

const downloadStatusText = computed(() => {
  if (downloadPhase.value === 'downloading') {
    return t.value.updateDialog.downloadProgress.replace('{progress}', downloadProgress.value)
  }
  const messageKey = DOWNLOAD_PHASE_MESSAGE_KEYS[downloadPhase.value]
  if (messageKey) return t.value.updateDialog[messageKey]
  return t.value.updateDialog.downloading
})

const getDownloadErrorMessage = (error) => {
  const code = error && typeof error === 'object' ? error.code : ''
  const messageKey = DOWNLOAD_ERROR_MESSAGE_KEYS[code] || 'downloadFailed'

  console.error('Download update failed', error)
  return t.value.updateDialog[messageKey]
}

const applyDownloadProgress = (payload = {}) => {
  if (payload.progress !== undefined) downloadProgress.value = payload.progress
  if (payload.phase) downloadPhase.value = payload.phase
  downloadSource.value = payload.source || ''
}

const resetDownloadState = () => {
  isDownloading.value = false
  downloadProgress.value = 0
  downloadPhase.value = 'idle'
  downloadSource.value = ''
  downloadedFilePath.value = ''
}

const installSteps = computed(() => [
  { key: 'downloaded', label: t.value.updateDialog.installStepDownloaded },
  { key: 'building-command', label: t.value.updateDialog.installStepPreparingHelper },
  { key: 'launching-installer', label: t.value.updateDialog.installStepLaunching },
  { key: 'installer-started', label: t.value.updateDialog.installStepBackground },
  { key: 'app-exiting', label: t.value.updateDialog.installStepExiting },
])

const installStageRanks = {
  idle: -1,
  preparing: 0,
  downloaded: 0,
  'building-command': 1,
  'launching-installer': 2,
  'installer-started': 3,
  'app-exiting': 4,
  failed: -1,
}

const currentInstallRank = computed(() => installStageRanks[installStage.value] ?? 0)

const currentInstallMessage = computed(() => {
  const messages = {
    idle: t.value.updateDialog.preparingInstall,
    preparing: t.value.updateDialog.installPreparing,
    downloaded: t.value.updateDialog.installPreparing,
    'building-command': t.value.updateDialog.installPreparingHelper,
    'launching-installer': t.value.updateDialog.installLaunching,
    'installer-started': t.value.updateDialog.installStartedBackground,
    'app-exiting': t.value.updateDialog.installExiting,
    failed: t.value.updateDialog.installFailed,
  }
  return messages[installStage.value] || t.value.updateDialog.preparingInstall
})

const getInstallStepStatus = (index) => {
  if (installStage.value === 'failed') {
    const failedRank = installFailedRank.value ?? Math.max(currentInstallRank.value, 0)
    if (index < failedRank) return 'done'
    if (index === failedRank) return 'failed'
    return 'pending'
  }
  if (index < currentInstallRank.value) return 'done'
  if (index === currentInstallRank.value) return 'active'
  return 'pending'
}

const clearInstallProgressListener = () => {
  if (installProgressUnlisten) {
    installProgressUnlisten()
    installProgressUnlisten = null
  }
}

const clearInstallWaitTimer = () => {
  if (installWaitTimer) {
    clearInterval(installWaitTimer)
    installWaitTimer = null
  }
}

const markInstallProgressSeen = () => {
  installLastEventAt.value = Date.now()
  installWaitHint.value = ''
}

const startInstallWaitTimer = () => {
  clearInstallWaitTimer()
  markInstallProgressSeen()
  installWaitTimer = setInterval(() => {
    if (!isInstalling.value || installStage.value === 'failed') return
    const elapsedMs = Date.now() - installLastEventAt.value
    if (elapsedMs >= 60000) {
      installWaitHint.value = t.value.updateDialog.installLongWaitHint
    } else if (elapsedMs >= 15000) {
      installWaitHint.value = t.value.updateDialog.installWaitHint
    } else {
      installWaitHint.value = ''
    }
  }, 1000)
}

const resetInstallProgress = () => {
  installStage.value = 'idle'
  installDetail.value = ''
  installWaitHint.value = ''
  installLastEventAt.value = 0
  installFailedRank.value = null
  clearInstallWaitTimer()
}

// ---------- 源码模式：同步 fork + 本地构建 ----------

const SOURCE_STAGE_RANKS = Object.freeze({
  idle: -1,
  fetch: 0,
  push: 1,
  merge: 2,
  submodule: 3,
  configure: 4,
  build: 5,
  package: 6,
  complete: 7,
})

const SOURCE_STAGE_DEFAULT_KEYS = Object.freeze({
  fetch: 'sourceStageFetch',
  push: 'sourceStagePush',
  merge: 'sourceStageMerge',
  submodule: 'sourceStageSubmodule',
  configure: 'sourceStageConfigure',
  build: 'sourceStageBuild',
  package: 'sourceStagePackage',
  complete: 'sourceStageComplete',
})

const sourceSteps = computed(() => [
  { key: 'fetch', label: t.value.updateDialog.sourceStepFetch },
  { key: 'push', label: t.value.updateDialog.sourceStepPush },
  { key: 'merge', label: t.value.updateDialog.sourceStepMerge },
  { key: 'submodule', label: t.value.updateDialog.sourceStepSubmodule },
  { key: 'configure', label: t.value.updateDialog.sourceStepConfigure },
  { key: 'build', label: t.value.updateDialog.sourceStepBuild },
  { key: 'package', label: t.value.updateDialog.sourceStepPackage },
])

const sourceStatusText = computed(() => {
  if (sourceError.value) return sourceError.value
  if (sourceStatusMessage.value) return sourceStatusMessage.value
  const key = SOURCE_STAGE_DEFAULT_KEYS[sourceStage.value]
  return key ? t.value.updateDialog[key] : t.value.updateDialog.sourceIdle
})

const sourceLogTail = computed(() => sourceLogLines.value.slice(-12).join('\n'))

const getSourceStepStatus = (index) => {
  const rank = SOURCE_STAGE_RANKS[sourceStage.value] ?? -1
  if (sourceError.value) {
    if (index < rank) return 'done'
    if (index === rank) return 'failed'
    return 'pending'
  }
  if (index < rank) return 'done'
  if (index === rank) return 'active'
  return 'pending'
}

const appendSourceLog = (line) => {
  const next = sourceLogLines.value.concat(String(line))
  sourceLogLines.value = next.length > SOURCE_LOG_MAX ? next.slice(next.length - SOURCE_LOG_MAX) : next
}

const clearSourceProgressListener = () => {
  if (sourceProgressUnlisten) {
    sourceProgressUnlisten()
    sourceProgressUnlisten = null
  }
}

const resetSourceState = () => {
  isSourceStarting.value = false
  isSourceRunning.value = false
  sourceStage.value = 'idle'
  sourceStatusMessage.value = ''
  sourceError.value = ''
  sourceLogLines.value = []
  clearSourceProgressListener()
}

// ---------- 发布版模式下的本地源码目录入口 ----------

const sourceSectionOpen = ref(true)
const sourceDirBusy = ref(false)
const sourceRechecking = ref(false)
const sourcePickerAvailable = ref(false)
const sourceDirState = ref({ valid: false, path: '', source: 'none', branch: '', reason: '' })

const showSourcePicker = computed(
  () =>
    sourcePickerAvailable.value &&
    !isSourceBuild.value &&
    !isDownloading.value &&
    !isInstalling.value &&
    !isSourceRunning.value
)

const applySourceDirStatus = (status) => {
  sourceDirState.value = {
    valid: !!status?.valid,
    path: status?.path || '',
    source: status?.source || 'none',
    branch: status?.branch || '',
    reason: status?.reason || '',
  }
}

const refreshSourceDirStatus = async () => {
  sourceDirBusy.value = true
  try {
    const { invoke } = await getTauriApis()
    const status = await invoke('validate_sunshine_source_dir', { path: null })
    sourcePickerAvailable.value = true
    applySourceDirStatus(status)
    if (status?.valid) sourceSectionOpen.value = false
  } catch {
    // 非 Tauri 环境下隐藏整个源码入口
    sourcePickerAvailable.value = false
  } finally {
    sourceDirBusy.value = false
  }
}

const recheckSourceUpdate = async () => {
  sourceRechecking.value = true
  try {
    const { invoke } = await getTauriApis()
    const result = await invoke('check_for_updates')
    if (result) emit('update-info', result)
  } catch (error) {
    ElMessage.error(
      t.value.updateDialog.sourceModeRecheckFailed.replace('{error}', String(error?.message ?? error))
    )
  } finally {
    sourceRechecking.value = false
  }
}

const browseSourceDir = async () => {
  sourceDirBusy.value = true
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const dir = await open({ directory: true, multiple: false })
    if (!dir) return
    const { invoke } = await getTauriApis()
    await invoke('set_sunshine_source_dir', { path: dir })
    const status = await invoke('validate_sunshine_source_dir', { path: dir })
    applySourceDirStatus(status)
    if (status?.valid) await recheckSourceUpdate()
  } catch (error) {
    ElMessage.error(String(error?.message ?? error))
  } finally {
    sourceDirBusy.value = false
  }
}

const clearSourceDir = async () => {
  sourceDirBusy.value = true
  try {
    const { invoke } = await getTauriApis()
    await invoke('set_sunshine_source_dir', { path: null })
    const status = await invoke('validate_sunshine_source_dir', { path: null })
    applySourceDirStatus(status)
    sourceSectionOpen.value = true
  } catch (error) {
    ElMessage.error(String(error?.message ?? error))
  } finally {
    sourceDirBusy.value = false
  }
}

const handleStartSourceUpdate = async () => {
  resetSourceState()
  isSourceStarting.value = true
  isSourceRunning.value = true
  let progressUnlisten = null

  try {
    const { invoke, listen } = await getTauriApis()
    progressUnlisten = await listen('source-update-progress', (event) => {
      const payload = event.payload || {}
      if (payload.stage && SOURCE_STAGE_RANKS[payload.stage] !== undefined) {
        sourceStage.value = payload.stage
      }
      if (payload.message) sourceStatusMessage.value = payload.message
      if (payload.line) appendSourceLog(payload.line)
    })
    sourceProgressUnlisten = progressUnlisten

    const artifactPath = await invoke('start_source_update')
    sourceStage.value = 'complete'
    sourceStatusMessage.value = ''
    downloadedFilePath.value = artifactPath
    isSourceRunning.value = false
    ElMessage.success(t.value.updateDialog.sourceBuildComplete)
    await new Promise((resolve) => setTimeout(resolve, 800))
    await handleInstall(artifactPath, 'nsis')
  } catch (error) {
    sourceError.value = String(error?.message ?? error)
  } finally {
    isSourceRunning.value = false
    isSourceStarting.value = false
    if (progressUnlisten) {
      await progressUnlisten()
    }
    sourceProgressUnlisten = null
  }
}

const getTauriApis = async () => {
  const [{ invoke }, { listen }] = await Promise.all([import('@tauri-apps/api/core'), import('@tauri-apps/api/event')])
  return { invoke, listen }
}

// 打开对话框时检查 temp 目录是否已有完整下载的安装包（上次取消安装或关闭应用时遗留）
let cacheCheckId = 0
const checkCachedInstaller = async () => {
  if (isLatest.value || !props.updateInfo?.download_name) return
  const checkId = ++cacheCheckId
  const filename = props.updateInfo.download_name
  try {
    const { invoke } = await getTauriApis()
    const cached = await invoke('check_cached_update', {
      filename,
      expectedSize: props.updateInfo.download_size || null,
      expectedSha256: props.updateInfo.download_sha256 || null,
      assetType: props.updateInfo.download_type || null,
    })
    if (
      cached &&
      checkId === cacheCheckId &&
      visible.value &&
      props.updateInfo?.download_name === filename &&
      !isDownloading.value &&
      !isInstalling.value
    ) {
      downloadedFilePath.value = cached
    }
  } catch (error) {
    console.warn('Failed to check cached installer:', error)
  }
}

const handleDownload = async () => {
  const downloadUrl = props.updateInfo?.download_url
  if (!downloadUrl) {
    ElMessage.warning(t.value.updateDialog.noDownloadUrl)
    return
  }

  resetDownloadState()
  isDownloading.value = true
  let progressUnlisten = null

  try {
    const { invoke, listen } = await getTauriApis()

    progressUnlisten = await listen('download-progress', (event) => {
      applyDownloadProgress(event.payload)
    })

    const filename = props.updateInfo.download_name || `sunshine-update-${props.updateInfo.version}.exe`
    const result = await invoke('download_update', {
      url: downloadUrl,
      fallbackUrl: props.updateInfo.download_fallback_url || null,
      filename,
      expectedSize: props.updateInfo.download_size || null,
      expectedSha256: props.updateInfo.download_sha256 || null,
      assetType: props.updateInfo.download_type || null,
    })
    await progressUnlisten()
    progressUnlisten = null

    if (result.success) {
      downloadedFilePath.value = result.file_path
      downloadProgress.value = 100
      downloadPhase.value = 'complete'
      if (!result.cached) {
        ElMessage.success(t.value.updateDialog.downloadComplete)
        await new Promise((resolve) => setTimeout(resolve, 1000))
      }
      isDownloading.value = false
      await handleInstall(result.file_path)
    } else {
      console.error('Download update returned an unsuccessful result', result)
      resetDownloadState()
      ElMessage.error(t.value.updateDialog.downloadFailed)
    }
  } catch (error) {
    resetDownloadState()
    ElMessage.error(getDownloadErrorMessage(error))
  } finally {
    if (progressUnlisten) {
      await progressUnlisten()
    }
    isDownloading.value = false
  }
}

const handleInstall = async (filePath, installerKind = null) => {
  try {
    const isSource = isSourceBuild.value || installerKind === 'nsis'
    await ElMessageBox.confirm(
      isSource ? t.value.updateDialog.sourceInstallConfirm : t.value.updateDialog.installConfirm,
      t.value.updateDialog.installTitle,
      { confirmButtonText: t.value.updateDialog.installConfirmBtn, cancelButtonText: t.value.updateDialog.cancelBtn, type: 'warning' }
    )

    isInstalling.value = true
    resetInstallProgress()
    installStage.value = 'downloaded'
    startInstallWaitTimer()

    const { invoke, listen } = await getTauriApis()
    clearInstallProgressListener()
    installProgressUnlisten = await listen('install-progress', (event) => {
      const payload = event.payload || {}
      if (payload.stage) {
        if (payload.stage === 'failed') {
          installFailedRank.value = Math.max(currentInstallRank.value, 0)
        } else {
          installFailedRank.value = null
        }
        installStage.value = payload.stage
      }
      installDetail.value = payload.detail || ''
      markInstallProgressSeen()
      if (payload.terminal) clearInstallWaitTimer()
    })

    await invoke('install_update', {
      filePath,
      targetVersion: props.updateInfo?.version || null,
      installerKind,
      expectedSize: props.updateInfo?.download_size || null,
      expectedSha256: props.updateInfo?.download_sha256 || null,
    })

    ElMessage.success(t.value.updateDialog.installStarted)
  } catch (error) {
    if (error === 'cancel') {
      isInstalling.value = false
      resetInstallProgress()
      clearInstallProgressListener()
      return
    }
    installFailedRank.value = Math.max(currentInstallRank.value, 0)
    installStage.value = 'failed'
    installDetail.value = String(error)
    clearInstallWaitTimer()
    clearInstallProgressListener()
    ElMessage.error(t.value.updateDialog.installError.replace('{error}', error))
    isInstalling.value = false
  }
}

const handleOpenBrowser = async () => {
  const releasePage = props.updateInfo?.release_page
  if (!releasePage) {
    ElMessage.warning(t.value.updateDialog.noReleasePage)
    return
  }

  try {
    const { invoke } = await getTauriApis()
    await invoke('open_external_url', { url: releasePage })
  } catch (error) {
    ElMessage.error(t.value.updateDialog.openBrowserError.replace('{error}', error))
  }
}

const handleCancel = () => {
  visible.value = false
  emit('close')
}

const handleSkipVersion = () => {
  const version = props.updateInfo?.version
  if (version) {
    emit('skip-version', version)
  }
  visible.value = false
  emit('close')
}

const resetState = () => {
  resetDownloadState()
  isInstalling.value = false
  resetInstallProgress()
  clearInstallProgressListener()
  resetSourceState()
}

// 对话框由父级 v-if 控制：每次打开都是全新挂载且 modelValue 初始即 true，
// watch(modelValue) 不会触发，挂载时直接拉一次当前源码目录状态。
onMounted(() => {
  if (props.modelValue) refreshSourceDirStatus()
})

onBeforeUnmount(() => {
  clearInstallWaitTimer()
  clearInstallProgressListener()
  clearSourceProgressListener()
})

watch(
  () => props.modelValue,
  (newVal) => {
    if (newVal) {
      resetState()
      checkCachedInstaller()
      refreshSourceDirStatus()
    }
  }
)
</script>

<style scoped lang="less">
@border-radius: 8px;

.update-dialog-content {
  padding: 0;
}

.release-notes {
  margin-bottom: 0;

  .notes-content {
    max-height: 400px;
    overflow-y: auto;
    padding: 14px 18px;
    background: #f5f7fa;
    border-radius: @border-radius;
    color: #4a4a4a;
    font-size: 14px;
    line-height: 1.7;

    &::-webkit-scrollbar {
      width: 6px;
    }

    &::-webkit-scrollbar-track {
      background: transparent;
      border-radius: 3px;
    }

    &::-webkit-scrollbar-thumb {
      background: #c0c4cc;
      border-radius: 3px;

      &:hover {
        background: #909399;
      }
    }

    :deep(h2),
    :deep(h3) {
      margin: 12px 0 8px;
      font-weight: 600;
      color: #303133;
    }

    :deep(p) {
      margin: 8px 0;
    }

    :deep(ul),
    :deep(ol) {
      margin: 8px 0;
      padding-left: 24px;
    }

    :deep(img[src^="https://github.com/"][src*=".png?size=40"]) {
      width: 40px;
      height: 40px;
    }

    :deep(li) {
      margin: 4px 0;
    }

    :deep(code) {
      padding: 2px 6px;
      background: #e4e7ed;
      border-radius: 3px;
    }

    :deep(a) {
      color: #409eff;
      text-decoration: none;
      word-break: break-all;

      &:hover {
        text-decoration: underline;
      }
    }
  }
}

.source-dir-panel {
  margin-top: 14px;
  border: 1px solid #dcdfe6;
  border-radius: @border-radius;
  background: #f8fafc;
  overflow: hidden;

  .source-dir-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    cursor: pointer;
    user-select: none;

    &:hover {
      background: #f0f2f5;
    }
  }

  .source-dir-icon {
    color: #409eff;
  }

  .source-dir-title {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
    color: #303133;
  }

  .source-dir-arrow {
    color: #909399;
    transition: transform 0.2s;

    &.open {
      transform: rotate(180deg);
    }
  }

  .source-dir-body {
    padding: 0 14px 12px;
  }

  .source-dir-desc {
    margin: 4px 0 10px;
    font-size: 12px;
    line-height: 1.6;
    color: #909399;
  }

  .source-dir-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .source-dir-input {
    flex: 1;
    min-width: 0;
    padding: 6px 10px;
    border: 1px solid #dcdfe6;
    border-radius: 6px;
    background: #fff;
    color: #606266;
    font-size: 12px;
    font-family: Consolas, Menlo, monospace;

    &:focus {
      outline: none;
      border-color: #409eff;
    }
  }

  .source-dir-hint {
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 1.5;

    &.muted {
      color: #909399;
    }

    &.ok {
      color: #67c23a;
    }

    &.bad {
      color: #f56c6c;
      word-break: break-all;
    }
  }

  .source-dir-meta {
    color: #909399;
  }
}

.install-ready-panel {
  margin-top: 14px;
  margin-bottom: 4px;
  padding: 12px 18px;
  border: 1px solid #d9ecff;
  border-radius: @border-radius;
  background: #ecf5ff;
}

.source-progress {
  margin-top: 14px;
  margin-bottom: 4px;
  padding: 14px 16px;
  border: 1px solid #dcdfe6;
  border-radius: @border-radius;
  background: #f8fafc;

  .download-status {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    min-height: 28px;
    margin-bottom: 10px;
  }

  .download-spinner {
    width: 14px;
    height: 14px;
    flex: 0 0 14px;
    border: 2px solid #c6e2ff;
    border-top-color: #409eff;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }

  .download-complete-mark {
    width: 18px;
    height: 18px;
    color: #67c23a;
    font-size: 18px;
    font-weight: 700;
    line-height: 18px;
  }

  .progress-text {
    margin: 0;
    color: #606266;
    font-size: 13px;
    line-height: 1.5;
  }
}

.source-log {
  max-height: 150px;
  margin: 10px 0 0;
  padding: 8px 10px;
  overflow: auto;
  border-radius: 6px;
  background: #1e1e1e;
  color: #d4d4d4;
  font-family: Consolas, Menlo, monospace;
  font-size: 11px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
}

.source-error {
  margin: 10px 0 0;
  padding: 8px 10px;
  border-radius: 6px;
  background: #fef0f0;
  color: #f56c6c;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
}

.download-progress {
  margin-top: 14px;
  margin-bottom: 4px;

  .download-status {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    min-height: 28px;
    margin-bottom: 10px;
  }

  .download-spinner {
    width: 14px;
    height: 14px;
    flex: 0 0 14px;
    border: 2px solid #c6e2ff;
    border-top-color: #409eff;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }

  .download-complete-mark {
    width: 18px;
    height: 18px;
    color: #67c23a;
    font-size: 18px;
    font-weight: 700;
    line-height: 18px;
  }

  .download-source-chip {
    display: inline-flex;
    align-items: center;
    min-height: 20px;
    padding: 1px 9px;
    border: 1px solid #b3d8ff;
    border-radius: 999px;
    background: #ecf5ff;
    color: #409eff;
    font-size: 12px;
    font-weight: 600;
    line-height: 18px;
    white-space: nowrap;
  }

  .progress-text {
    text-align: center;
    color: #909399;
    font-size: 14px;
    margin: 0;
  }
}

.install-progress-panel {
  margin-top: 16px;
  padding: 16px;
  border: 1px solid #dcdfe6;
  border-radius: @border-radius;
  background: #f8fafc;
}

.install-status {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  margin-bottom: 14px;
}

.install-spinner {
  width: 18px;
  height: 18px;
  flex: 0 0 18px;
  margin-top: 2px;
  border: 2px solid #c6e2ff;
  border-top-color: #409eff;
  border-radius: 50%;
  animation: spin 0.9s linear infinite;

  &.failed {
    border-color: #f56c6c;
    animation: none;
  }
}

.install-current {
  margin: 0;
  color: #303133;
  font-size: 14px;
  font-weight: 600;
}

.install-detail,
.install-note,
.install-wait-hint {
  margin: 6px 0 0;
  color: #606266;
  font-size: 13px;
  line-height: 1.5;
}

.install-wait-hint {
  color: #b88230;
}

.install-steps {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(118px, 1fr));
  gap: 8px;
  margin: 0;
  padding: 0;
  list-style: none;

  li {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: 6px;
    color: #909399;
    font-size: 12px;
    line-height: 1.3;
  }

  .step-dot {
    width: 10px;
    height: 10px;
    flex: 0 0 10px;
    border: 2px solid #c0c4cc;
    border-radius: 50%;
    background: #fff;
  }

  .step-label {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  li.done {
    color: #409eff;

    .step-dot {
      border-color: #409eff;
      background: #409eff;
    }
  }

  li.active {
    color: #303133;
    font-weight: 600;

    .step-dot {
      border-color: #409eff;
      background: #ecf5ff;
    }
  }

  li.failed {
    color: #f56c6c;

    .step-dot {
      border-color: #f56c6c;
      background: #fef0f0;
    }
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.dialog-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
}
</style>
