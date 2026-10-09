<script setup lang="ts">
// 设置对话框：资料库位置、完整备份、恢复到新目录、模板下载。
// 由侧栏底部入口打开。
import { ref, watch } from 'vue'
import { Download, Folder, FolderOpened } from '@element-plus/icons-vue'
import { api } from '../ipc'
import { toastError, toastInfo, toastOk } from '../lib/toast'
import { THEMES, getCurrentThemeId, selectTheme } from '../lib/theme'

const visible = defineModel<boolean>({ default: false })

const libraryDir = ref('')
const busy = ref(false)
const backupDir = ref('')
const zipPath = ref('')
const restoreDir = ref('')
const lastBackup = ref('')

// AI 服务（OpenAI 兼容：智谱 GLM 默认 / DeepSeek / Ollama 均可）
const aiCfg = ref({
  base_url: 'https://open.bigmodel.cn/api/paas/v4',
  api_key: '',
  model: 'glm-5.3-flash',
  max_context_tokens: 120000,
  temperature: 0.3
})
const aiTesting = ref(false)
const aiSaving = ref(false)

async function loadAi() {
  try {
    aiCfg.value = await api.aiGetConfig()
  } catch {
    /* 使用默认值 */
  }
}

async function saveAi() {
  aiSaving.value = true
  try {
    await api.aiSetConfig({ ...aiCfg.value })
    toastOk('AI 配置已保存（仅存本机）')
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    aiSaving.value = false
  }
}

async function testAi() {
  aiTesting.value = true
  try {
    await api.aiSetConfig({ ...aiCfg.value })
    const r = await api.aiTestConnection()
    toastOk('连接成功，模型回复：' + (r.reply || '').slice(0, 20))
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    aiTesting.value = false
  }
}
const activeTheme = ref(getCurrentThemeId())

function onPickTheme(id: string) {
  const preset = selectTheme(id)
  activeTheme.value = preset.id
  toastOk(`已切换为「${preset.name}」`)
}

watch(visible, (v) => {
  if (v) {
    load()
    loadAi()
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
    <div class="section-title">外观</div>
    <div class="theme-grid">
      <button
        v-for="t in THEMES"
        :key="t.id"
        class="theme-card"
        :class="{ active: activeTheme === t.id }"
        :style="{ '--theme-color': t.color }"
        :title="`${t.name} · ${t.mood}（${t.origin}）`"
        :aria-label="`选择${t.name}主题`"
        :aria-pressed="activeTheme === t.id"
        @click="onPickTheme(t.id)"
      >
        <span class="theme-swatch" aria-hidden="true"></span>
        <span class="theme-name">{{ t.name }}</span>
      </button>
    </div>

    <div class="section-title">资料库</div>
    <p class="library-note">数据保存在应用安装目录之外，升级或重装不会清除文献。</p>
    <div class="library-location"><span>当前资料库</span><code class="dir-value">{{ libraryDir }}</code></div>

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

        <div class="section-title">AI 服务 <span class="section-sub">OpenAI 兼容：智谱 / DeepSeek / Ollama 均可</span></div>
    <div class="ai-grid">
      <el-input v-model="aiCfg.base_url" placeholder="https://open.bigmodel.cn/api/paas/v4">
        <template #prepend>Base URL</template>
      </el-input>
      <el-input v-model="aiCfg.model" placeholder="glm-5.3-flash">
        <template #prepend>模型</template>
      </el-input>
      <el-input v-model="aiCfg.api_key" type="password" show-password placeholder="API Key（仅存本机数据库，不进 git）">
        <template #prepend>API Key</template>
      </el-input>
      <el-input-number v-model="aiCfg.max_context_tokens" :min="8000" :max="1000000" :step="8000" controls-position="right" style="width: 100%" />
    </div>
    <div class="ai-actions">
      <el-button size="small" :loading="aiSaving" @click="saveAi">保存</el-button>
      <el-button size="small" type="primary" :loading="aiTesting" @click="testAi">测试连接</el-button>
    </div>

    <div class="section-title">AI 分析模板</div>
    <el-button :icon="Download" @click="downloadTemplate">下载 文献AI分析模板.md</el-button>
    <p v-if="!api.isTauri" class="hint">浏览器模式下目录请输入绝对路径（例如 D:\Backups）。</p>
  </el-dialog>
</template>

<style scoped>
.dir-value {
  font-size: 13px;
  word-break: break-all;
}

.ai-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px 12px;
  margin-bottom: 10px;
}

.ai-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.ai-actions .hint {
  margin: 0;
  max-width: 360px;
}

.section-title {
  font-weight: 600;
  font-size: 15px;
  margin: 20px 0 10px;
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(88px, 1fr));
  gap: 8px;
  margin-bottom: 20px;
}

.theme-card {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 48px;
  padding: 8px;
  border: 1px solid var(--app-line);
  border-radius: 6px;
  color: var(--app-text);
  cursor: pointer;
  background: var(--app-surface);
  transition: border-color 0.15s, background 0.15s;
}

.theme-card:hover {
  background: var(--app-rail);
  border-color: var(--app-subtle);
}

.theme-card.active {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}

.theme-card:focus-visible {
  outline: 2px solid var(--el-color-primary);
  outline-offset: 2px;
}

.theme-swatch {
  flex: none;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--theme-color);
}

.theme-name {
  font-size: 13px;
  font-weight: 600;
}

.library-note {
  margin: 0 0 10px;
  font-size: 13px;
  color: var(--app-muted);
}

.library-location {
  display: flex;
  align-items: baseline;
  gap: 16px;
  min-width: 0;
  padding: 10px 12px;
  border: 1px solid var(--app-line);
  border-radius: 6px;
  font-size: 13px;
}

.library-location > span {
  flex: none;
  color: var(--app-muted);
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

.hint {
  color: #6a737d;
  font-size: 12px;
  margin: 0 0 6px;
  line-height: 1.7;
}
</style>
