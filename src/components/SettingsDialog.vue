<script setup lang="ts">
// 设置对话框：资料库位置、完整备份、恢复到新目录、模板下载。
// 由侧栏底部入口打开。
import { ref, watch } from 'vue'
import { Download, Folder, FolderOpened } from '@element-plus/icons-vue'
import { api } from '../ipc'
import { toastError, toastInfo, toastOk } from '../lib/toast'
import { THEMES, getCurrentThemeId, selectTheme } from '../lib/theme'
import { MOOD_ICONS, MOOD_ICON_VIEWBOX } from '../lib/moodIcons'

const visible = defineModel<boolean>({ default: false })

const libraryDir = ref('')
const busy = ref(false)
const backupDir = ref('')
const zipPath = ref('')
const restoreDir = ref('')
const lastBackup = ref('')
const activeTheme = ref(getCurrentThemeId())

// 桌宠（仅桌面版）
const petEnabled = ref(false)
const petHasImage = ref(false)
const petLoading = ref(false)

async function loadPet() {
  if (!api.isTauri) return
  try {
    const cfg = await api.petGetConfig()
    if (cfg) {
      petEnabled.value = cfg.enabled
      petHasImage.value = cfg.has_image
    }
  } catch {
    /* 忽略 */
  }
}

async function togglePet(v: boolean) {
  petEnabled.value = v
  try {
    await api.petSetEnabled(v)
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
    const win = WebviewWindow.getByLabel('pet')
    if (win) {
      if (v) await win.show()
      else await win.hide()
    }
    toastOk(v ? '桌宠已开启（可在桌面拖动它）' : '桌宠已关闭')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function importPetImage() {
  try {
    const r = await api.pickFile(['png', 'jpg', 'jpeg', 'webp', 'gif'])
    if (!r?.file) {
      toastInfo('浏览器无法读取本地图片路径，请在桌面版中导入形象')
      return
    }
    await api.petSetImage(r.file)
    petHasImage.value = true
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
    const win = WebviewWindow.getByLabel('pet')
    if (win && petEnabled.value) {
      await win.hide()
      await win.show()
    }
    toastOk('形象已更新')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

function onPickTheme(id: string) {
  const preset = selectTheme(id)
  activeTheme.value = preset.id
  toastOk(`主题已切换为「${preset.name}」（${preset.mood}）`)
}

watch(visible, (v) => {
  if (v) {
    load()
    loadPet()
  }
})

async function load() {
  try {
    const s = await api.getSettings()
    libraryDir.value = s.library_dir
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function pickBackupDir() {
  const d = await api.pickDir()
  if (d) backupDir.value = d
}

async function pickZip() {
  const r = await api.pickFile(['zip'])
  if (r?.path) zipPath.value = r.path
  else if (r?.file) zipPath.value = r.file.name
}

async function pickRestoreDir() {
  const d = await api.pickDir()
  if (d) restoreDir.value = d
}

async function doBackup() {
  if (!backupDir.value.trim()) {
    toastInfo('请先选择备份保存目录')
    return
  }
  busy.value = true
  try {
    const r = await api.backup(backupDir.value.trim())
    lastBackup.value = r.zip_path
    toastOk(`备份完成：${r.papers} 篇文献，${(r.bytes / 1024 / 1024).toFixed(1)} MB`)
  } catch (e) {
    toastError(`备份失败：${String((e as Error).message || e)}`)
  } finally {
    busy.value = false
  }
}

async function doRestore() {
  if (!zipPath.value.trim()) {
    toastInfo('请先选择备份 ZIP 文件')
    return
  }
  ElMessage.warning('恢复中：先校验备份完整性，再切换资料库目录')
  busy.value = true
  try {
    const r = await api.restore(zipPath.value.trim(), restoreDir.value.trim() || null)
    libraryDir.value = r.library_dir
    r.warnings.forEach((w) => toastError(w))
    toastOk(`恢复完成：${r.papers} 篇文献，资料库已切换到新目录`)
  } catch (e) {
    toastError(`恢复失败：${String((e as Error).message || e)}`)
  } finally {
    busy.value = false
  }
}

async function downloadTemplate() {
  const t = await api.getTemplate()
  await api.saveText(t.filename, t.content)
  toastOk('模板已保存，可连同需求文档第 5 节规则一起交给外部 AI')
}
</script>

<template>
  <el-dialog v-model="visible" title="设置与备份" width="min(780px, 92vw)" :close-on-click-modal="false" destroy-on-close>
    <!-- 主题色 -->
    <!-- 桌宠（仅桌面版） -->
    <template v-if="api.isTauri">
      <div class="section-title">桌宠</div>
      <div class="pet-row">
        <el-switch v-model="petEnabled" :loading="petLoading" @change="togglePet" />
        <span class="pet-hint">开启后桌宠常驻桌面（拖动移动、单击互动、双击回主界面）</span>
        <el-button size="small" style="margin-left: auto" @click="importPetImage">
          {{ petHasImage ? '更换形象图片' : '导入形象图片' }}
        </el-button>
      </div>
      <p class="hint">形象可用任意 AI 生成的透明底 PNG（应用默认使用小兔子）；形象文件保存在资料库 pets 目录，随备份迁移。</p>
    </template>

    <div class="section-title">主题色</div>
    <div class="theme-grid">
      <button
        v-for="t in THEMES"
        :key="t.id"
        class="theme-card"
        :class="{ active: activeTheme === t.id }"
        :style="{ '--theme-color': t.color }"
        :title="`${t.name} · ${t.mood}（${t.origin}）`"
        @click="onPickTheme(t.id)"
      >
        <svg class="theme-icon" :viewBox="MOOD_ICON_VIEWBOX" aria-hidden="true">
          <g v-html="MOOD_ICONS[t.icon]" />
        </svg>
        <span class="theme-name">{{ t.name }}</span>
        <span class="theme-mood">{{ t.mood }}</span>
        <span v-if="activeTheme === t.id" class="theme-check">
          <svg :viewBox="MOOD_ICON_VIEWBOX" aria-hidden="true"><path d="M10 24 L20 34 L38 14" fill="none" stroke="currentColor" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </span>
      </button>
    </div>

    <el-alert
      type="info"
      :closable="false"
      show-icon
      class="gap"
      title="资料库独立于应用安装目录，包含 SQLite 数据库与托管 PDF；升级或重装应用不会影响这里的数据。"
    />
    <el-descriptions :column="1" border size="small" class="gap">
      <el-descriptions-item label="当前资料库">
        <code class="dir-value">{{ libraryDir }}</code>
      </el-descriptions-item>
    </el-descriptions>

    <div class="section-title">完整备份</div>
    <div class="op-row">
      <el-input v-model="backupDir" placeholder="选择或输入备份保存目录（可放其他磁盘）">
        <template #prefix><el-icon><Folder /></el-icon></template>
      </el-input>
      <el-button :icon="FolderOpened" @click="pickBackupDir">浏览…</el-button>
      <el-button type="primary" :loading="busy" @click="doBackup">开始备份</el-button>
    </div>
    <div v-if="lastBackup" class="last-backup">最近备份：<code>{{ lastBackup }}</code></div>
    <p class="hint">备份包含数据库与全部托管 PDF；备份期间会暂停资料库写入以保证一致。</p>

    <div class="section-title">恢复备份</div>
    <div class="op-row">
      <el-input v-model="zipPath" placeholder="备份 ZIP 文件路径">
        <template #prefix><el-icon><Folder /></el-icon></template>
      </el-input>
      <el-button @click="pickZip">选择…</el-button>
    </div>
    <div class="op-row">
      <el-input v-model="restoreDir" placeholder="恢复到哪个目录（留空自动创建）">
        <template #prefix><el-icon><FolderOpened /></el-icon></template>
      </el-input>
      <el-button @click="pickRestoreDir">浏览…</el-button>
      <el-button type="primary" :loading="busy" @click="doRestore">校验并恢复</el-button>
    </div>
    <p class="hint">恢复前会校验数据库完整性与 PDF 关联，校验通过才切换到新目录。</p>

    <div class="section-title">AI 分析模板</div>
    <el-button :icon="Download" @click="downloadTemplate">下载 文献AI分析模板.md</el-button>
    <p v-if="!api.isTauri" class="hint">浏览器模式下目录请输入绝对路径（例如 D:\Backups）。</p>
  </el-dialog>
</template>

<style scoped>
.gap {
  margin-bottom: 12px;
}

.dir-value {
  font-size: 13px;
  word-break: break-all;
}

.section-title {
  font-weight: 600;
  font-size: 14px;
  margin: 14px 0 8px;
}

/* 主题色卡片：整卡填充主题色，底部暗化渐变保证白色文字可读 */
.theme-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 8px;
  margin-bottom: 6px;
}

.theme-card {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: space-between;
  height: 108px;
  padding: 14px 3px 8px;
  border: 2px solid transparent;
  border-radius: 10px;
  color: #fff;
  cursor: pointer;
  overflow: hidden;
  transition: transform 0.15s, box-shadow 0.15s, border-color 0.15s;
  background-color: var(--theme-color);
  background-image: linear-gradient(180deg, rgba(0, 0, 0, 0) 55%, rgba(0, 0, 0, 0.34) 100%);
  /* 渐变按 padding-box 定位后会平铺进 2px 透明边框区，把最暗端画到卡片顶边形成黑线 */
  background-repeat: no-repeat;
}

.theme-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.22);
}

.theme-card.active {
  border-color: #fff;
  box-shadow: 0 0 0 2px var(--el-color-primary-light-5), 0 4px 14px rgba(0, 0, 0, 0.22);
}

.theme-icon {
  width: 24px;
  height: 24px;
  filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.25));
}

.theme-name {
  font-size: 12.5px;
  font-weight: 600;
  line-height: 1.4;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
}

.theme-mood {
  font-size: 10.5px;
  line-height: 1.4;
  opacity: 0.95;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
}

.theme-check {
  position: absolute;
  top: 5px;
  right: 5px;
  width: 17px;
  height: 17px;
  border-radius: 50%;
  background: #fff;
  color: var(--el-color-primary);
  display: flex;
  align-items: center;
  justify-content: center;
}

.theme-check svg {
  width: 10px;
  height: 10px;
}

.op-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.op-row .el-input {
  flex: 1;
  min-width: 220px;
}

.last-backup {
  font-size: 12px;
  color: #6a737d;
  margin: 2px 0 6px;
}

.pet-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 6px;
}

.pet-hint {
  font-size: 12.5px;
  color: #4b5058;
}

.hint {
  color: #6a737d;
  font-size: 12px;
  margin: 0 0 6px;
  line-height: 1.7;
}
</style>
