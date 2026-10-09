<script setup lang="ts">
// 汇报大纲中心：勾选文献 → AI 生成组会汇报大纲（含编辑位）→ 可编辑 → 保存/导出 Markdown。
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { Document, MagicStick } from '@element-plus/icons-vue'
import { api, type PaperListItem } from '../ipc'
import { useLibraryStore } from '../stores/library'
import { toastError, toastOk } from '../lib/toast'
import MarkdownEditor from '../components/MarkdownEditor.vue'

const router = useRouter()
const store = useLibraryStore()

const papers = ref<PaperListItem[]>([])
const selected = ref<Set<string>>(new Set())
const generating = ref(false)
const streamText = ref('')
const outlineMd = ref('')
const extra = ref('')
const savedOutlines = ref<{ name: string; md: string; ts: number }[]>([])

const comparablePapers = computed(() => papers.value.filter((p) => p.analysis_status !== 'none'))

async function refresh() {
  papers.value = await api.listPapers({ analysis: 'comparable' })
}

onMounted(async () => {
  await store.ensureLoaded().catch(() => undefined)
  await refresh().catch(() => undefined)
  // 从 localStorage 恢复已保存的大纲
  try {
    const raw = localStorage.getItem('litwright-outlines')
    if (raw) savedOutlines.value = JSON.parse(raw)
  } catch { /* ignore */ }
})

function toggle(id: string) {
  if (selected.value.has(id)) selected.value.delete(id)
  else selected.value.add(id)
  selected.value = new Set(selected.value)
}

const canGenerate = computed(() => selected.value.size >= 2 && !generating.value)

async function generate() {
  if (!canGenerate.value) return
  // Key 检查
  try {
    const cfg = await api.aiGetConfig()
    if (!cfg.api_key) {
      ElMessage.warning('请先到「设置与备份 → AI 服务」填写 API Key')
      return
    }
  } catch { /* 放行走后端报错 */ }

  generating.value = true
  streamText.value = ''
  outlineMd.value = ''
  try {
    const full = await api.aiGenerateOutline(
      Array.from(selected.value),
      extra.value,
      (d) => { streamText.value += d }
    )
    outlineMd.value = full
    toastOk('大纲生成完毕，请编辑后保存或导出')
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    generating.value = false
  }
}

function saveOutline() {
  if (!outlineMd.value.trim()) return
  const name = `大纲 ${new Date().toLocaleDateString()} ${selected.value.size} 篇`
  savedOutlines.value.unshift({ name, md: outlineMd.value, ts: Date.now() })
  localStorage.setItem('litwright-outlines', JSON.stringify(savedOutlines.value.slice(0, 20)))
  toastOk('大纲已保存到本地')
}

function loadOutline(o: { md: string }) {
  outlineMd.value = o.md
  toastOk('已加载保存的大纲')
}

function deleteOutline(i: number) {
  savedOutlines.value.splice(i, 1)
  localStorage.setItem('litwright-outlines', JSON.stringify(savedOutlines.value))
}

async function exportOutline() {
  if (!outlineMd.value.trim()) return
  const d = new Date()
  const filename = `组会大纲-${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, '0')}${String(d.getDate()).padStart(2, '0')}.md`
  await api.saveText(filename, outlineMd.value)
  toastOk('已导出 Markdown')
}
</script>

<template>
  <div class="outline-page">
    <header class="page-header">
      <el-button text @click="router.push('/')">← 文献库</el-button>
      <h1 class="page-title">汇报大纲</h1>
      <el-button v-if="outlineMd" :icon="Document" @click="exportOutline">导出 MD</el-button>
    </header>

    <!-- 选文献 -->
    <el-card shadow="never" class="card">
      <template #header>
        <span>选择文献（已选 {{ selected.size }} 篇，需 2 篇以上）</span>
        <el-button text size="small" style="float: right" @click="selected = new Set(comparablePapers.map(p => p.id))">全选</el-button>
      </template>
      <div v-if="comparablePapers.length === 0" class="dim">
        没有已分析的文献。请先通过「智能导入」或「AI 生成分析」为文献生成分析内容。
      </div>
      <div v-else class="paper-chips">
        <el-check-tag
          v-for="p in comparablePapers"
          :key="p.id"
          :checked="selected.has(p.id)"
          @change="toggle(p.id)"
        >{{ p.title.length > 30 ? p.title.slice(0, 30) + '…' : p.title }}</el-check-tag>
      </div>
      <div class="gen-row">
        <el-input
          v-model="extra"
          placeholder="补充说明（可选）：如 侧重方法对比 / 本周重点是 XX 实验"
          style="flex: 1"
          @keyup.enter="generate"
        />
        <el-button
          type="primary"
          size="large"
          :icon="MagicStick"
          :loading="generating"
          :disabled="!canGenerate"
          @click="generate"
        >
          {{ generating ? 'AI 生成中…' : '生成组会大纲' }}
        </el-button>
      </div>
      <div v-if="generating" class="stream-box">
        <pre class="stream-pre">{{ streamText }}</pre>
      </div>
    </el-card>

    <!-- 大纲编辑区 -->
    <el-card v-if="outlineMd && !generating" shadow="never" class="card">
      <template #header>
        <span>大纲编辑（「个人工作总结」等编辑位请自行填写）</span>
        <span style="float: right">
          <el-button text size="small" @click="saveOutline">保存</el-button>
        </span>
      </template>
      <MarkdownEditor v-model="outlineMd" mode="tab" placeholder="大纲内容会显示在这里，可直接编辑" />
    </el-card>

    <!-- 已保存大纲 -->
    <el-card v-if="savedOutlines.length" shadow="never" class="card">
      <template #header><span>已保存的大纲</span></template>
      <div v-for="(o, i) in savedOutlines" :key="o.ts" class="saved-row">
        <span class="saved-name">{{ o.name }}</span>
        <span style="flex: 1"></span>
        <el-button text size="small" @click="loadOutline(o)">加载</el-button>
        <el-button text size="small" type="danger" @click="deleteOutline(i)">删除</el-button>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.outline-page {
  height: 100%;
  overflow-y: auto;
  max-width: 920px;
  margin: 0 auto;
  padding: 16px 24px 60px;
}

.page-header {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 12px;
}

.page-title {
  flex: 1;
  font-size: 20px;
  margin: 0;
}

.card {
  margin-bottom: 16px;
}

.dim {
  color: #8f959e;
  font-size: 13px;
}

.paper-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 14px;
}

.gen-row {
  display: flex;
  gap: 10px;
  align-items: center;
}

.stream-box {
  margin-top: 12px;
  border: 1px solid #e2e5ea;
  border-radius: 6px;
  background: #f8fafc;
  padding: 10px 14px;
}

.stream-pre {
  max-height: 260px;
  overflow-y: auto;
  font-size: 12px;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-all;
  margin: 0;
}

.saved-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 0;
  border-bottom: 1px dashed #eceef2;
}

.saved-name {
  font-size: 13px;
  color: #4b5058;
}
</style>
