<script setup lang="ts">
// 导入对话框：单篇（步骤式）+ 批量（多选文件、同名自动配对、批量入库）。
// 以 Dialog 形式从文献库打开，成功后通知父组件刷新。
import { computed, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import {
  ArrowLeft,
  Document,
  Download,
  Files,
  Promotion,
  UploadFilled
} from '@element-plus/icons-vue'
import { api, type CommitRequest, type ImportPreview } from '../ipc'
import { useLibraryStore } from '../stores/library'
import { toastError, toastOk } from '../lib/toast'
import MdRender from './MdRender.vue'

const visible = defineModel<boolean>({ default: false })
const emit = defineEmits<{ (e: 'imported'): void }>()

const store = useLibraryStore()
const FIXED_SECTIONS = ['一句话总结', '研究问题', '研究方法', '样本与数据', '主要发现', '创新点', '局限性', '阅读重点']

const activeTab = ref<'single' | 'batch'>('single')

watch(visible, (v) => {
  if (v) {
    store.ensureLoaded().catch(() => undefined)
    activeTab.value = 'single'
    resetSingle()
    resetBatch()
  } else {
    resetSingle()
    resetBatch()
  }
})

async function downloadTemplate() {
  const t = await api.getTemplate()
  await api.saveText(t.filename, t.content)
  toastOk('模板已保存，可连同约定一起交给外部 AI')
}

/* ==================== 单篇模式 ==================== */
const step = ref(0)
const pdfPath = ref('')
const pdfFile = ref<File | null>(null)
const mdPath = ref('')
const mdFile = ref<File | null>(null)
const mdPasted = ref('')
const showPaste = ref(false)
const preview = ref<ImportPreview | null>(null)
const analyzing = ref(false)
const committing = ref(false)
const pickedMdText = ref('')

const form = ref({
  title: '',
  authors: '',
  year: null as number | null,
  doi: '',
  tags: [] as string[],
  project: '',
  updatePaperId: '',
  replacePdf: false
})

function resetSingle() {
  step.value = 0
  preview.value = null
  pdfPath.value = ''
  pdfFile.value = null
  mdPath.value = ''
  mdFile.value = null
  mdPasted.value = ''
  pickedMdText.value = ''
  form.value = { title: '', authors: '', year: null, doi: '', tags: [], project: '', updatePaperId: '', replacePdf: false }
}

async function pickPdf() {
  const r = await api.pickFile(['pdf'])
  if (!r) return
  if (r.path) {
    pdfPath.value = r.path
    pdfFile.value = null
  } else if (r.file) {
    pdfFile.value = r.file
    pdfPath.value = r.file.name
  }
}

async function pickMd() {
  const r = await api.pickFile(['md', 'markdown', 'txt'])
  if (!r) return
  if (r.path) {
    mdPath.value = r.path
    mdFile.value = null
    pickedMdText.value = ''
  } else if (r.file) {
    mdFile.value = r.file
    mdPath.value = r.file.name
    pickedMdText.value = await r.file.text()
  }
}

const canAnalyze = computed(() => Boolean(pdfPath.value || mdPath.value || mdPasted.value.trim()))

async function analyze() {
  if (!canAnalyze.value) return
  analyzing.value = true
  try {
    preview.value = await api.analyzeImport({
      pdfPath: pdfPath.value || null,
      pdfFile: pdfFile.value,
      mdPath: mdPath.value || null,
      mdFile: mdFile.value,
      mdText: mdPasted.value.trim() || null
    })
    const meta = preview.value.md?.meta
    form.value.title = meta?.title || ''
    form.value.authors = (meta?.authors || []).join(', ')
    form.value.year = meta?.year ?? null
    form.value.doi = meta?.doi || ''
    form.value.tags = meta?.tags || []
    form.value.updatePaperId = ''
    form.value.replacePdf = false
    step.value = 1
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    analyzing.value = false
  }
}

const foundCount = computed(() => {
  const md = preview.value?.md
  if (!md) return 0
  return FIXED_SECTIONS.filter((s) => md.sections.some((x) => x.title === s)).length
})

const canCommit = computed(() => Boolean(preview.value && !committing.value && form.value.title.trim()))

async function commit() {
  if (!canCommit.value || !preview.value) return
  committing.value = true
  try {
    const req: CommitRequest = {
      temp_token: preview.value.temp_token,
      title: form.value.title.trim(),
      authors: form.value.authors.split(/[,，;；]/).map((x) => x.trim()).filter(Boolean),
      year: form.value.year,
      doi: form.value.doi.trim(),
      tags: form.value.tags,
      project: form.value.project.trim() || null,
      update_paper_id: form.value.updatePaperId || null,
      replace_pdf: form.value.replacePdf
    }
    const result = await api.commitImport(req)
    result.warnings.forEach((w) => toastError(w))
    toastOk(form.value.updatePaperId ? '分析已更新' : '导入成功')
    await store.refresh()
    emit('imported')
    visible.value = false
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    committing.value = false
  }
}

/* ==================== 批量模式 ==================== */
interface PickedFile {
  name: string
  path?: string
  file?: File
}

interface BatchRow {
  key: string
  stem: string
  pdf?: PickedFile
  md?: PickedFile
  preview?: ImportPreview
  state: 'pending' | 'importing' | 'ok' | 'fail'
  error?: string
}

const batchPdfList = ref<PickedFile[]>([])
const batchMdList = ref<PickedFile[]>([])
const batchRows = ref<BatchRow[]>([])
const batchProject = ref('')
const batchAnalyzing = ref(false)
const batchImporting = ref(false)
const batchProgress = ref({ done: 0, total: 0 })
const batchDone = ref(false)

function resetBatch() {
  batchPdfList.value = []
  batchMdList.value = []
  batchRows.value = []
  batchProject.value = ''
  batchAnalyzing.value = false
  batchImporting.value = false
  batchProgress.value = { done: 0, total: 0 }
  batchDone.value = false
}

const stem = (name: string) => name.replace(/\.[^.]+$/, '').toLowerCase()

async function pickBatchPdfs() {
  const r = await api.pickFiles(['pdf'])
  if (!r) return
  const picked: PickedFile[] = r.paths
    ? r.paths.map((p) => ({ name: p.split(/[\\/]/).pop() || p, path: p }))
    : (r.files || []).map((f) => ({ name: f.name, file: f }))
  const exist = new Set(batchPdfList.value.map((x) => x.name))
  batchPdfList.value.push(...picked.filter((p) => !exist.has(p.name)))
  rebuildRows()
}

async function pickBatchMds() {
  const r = await api.pickFiles(['md', 'markdown', 'txt'])
  if (!r) return
  const picked: PickedFile[] = r.paths
    ? r.paths.map((p) => ({ name: p.split(/[\\/]/).pop() || p, path: p }))
    : (r.files || []).map((f) => ({ name: f.name, file: f }))
  const exist = new Set(batchMdList.value.map((x) => x.name))
  batchMdList.value.push(...picked.filter((p) => !exist.has(p.name)))
  rebuildRows()
}

function rebuildRows() {
  // 同名（不含扩展名）自动配对；剩余的按 PDF-only / MD-only 列出
  const rows: BatchRow[] = []
  const mdByStem = new Map(batchMdList.value.map((m) => [stem(m.name), m]))
  const usedMd = new Set<string>()
  for (const pdf of batchPdfList.value) {
    const s = stem(pdf.name)
    const md = mdByStem.get(s)
    if (md) usedMd.add(md.name)
    rows.push({ key: `p:${s}`, stem: s, pdf, md, state: 'pending' })
  }
  for (const md of batchMdList.value) {
    if (!usedMd.has(md.name)) {
      rows.push({ key: `m:${stem(md.name)}`, stem: stem(md.name), md, state: 'pending' })
    }
  }
  batchRows.value = rows
  batchDone.value = false
}

function removeRow(key: string) {
  const row = batchRows.value.find((r) => r.key === key)
  if (!row) return
  if (row.pdf) batchPdfList.value = batchPdfList.value.filter((p) => p.name !== row.pdf!.name)
  if (row.md) batchMdList.value = batchMdList.value.filter((m) => m.name !== row.md!.name)
  rebuildRows()
}

const rowStatus = (r: BatchRow): { text: string; type: 'success' | 'warning' | 'info' } => {
  if (r.state === 'ok') return { text: '已导入', type: 'success' }
  if (r.state === 'fail') return { text: '失败', type: 'warning' }
  if (r.pdf && r.md) return { text: 'PDF+MD', type: 'success' }
  if (r.pdf) return { text: '仅 PDF', type: 'info' }
  return { text: '仅 MD', type: 'warning' }
}

const batchReady = computed(
  () => batchRows.value.length > 0 && !batchImporting.value && !batchAnalyzing.value
)

const batchHasAnalysis = (r: BatchRow) => {
  const md = r.preview?.md
  if (!md) return null
  return FIXED_SECTIONS.filter((s) => md.sections.some((x) => x.title === s)).length
}

async function importAll() {
  if (!batchReady.value) return
  batchImporting.value = true
  batchProgress.value = { done: 0, total: batchRows.value.length }
  let okCount = 0
  const fails: string[] = []
  for (const row of batchRows.value) {
    if (row.state === 'ok') {
      batchProgress.value.done++
      continue
    }
    row.state = 'importing'
    try {
      // 分析（暂存文件）
      const p = await api.analyzeImport({
        pdfPath: row.pdf?.path || null,
        pdfFile: row.pdf?.file || null,
        mdPath: row.md?.path || null,
        mdFile: row.md?.file || null,
        mdText: null
      })
      row.preview = p
      const meta = p.md?.meta
      const title = meta?.title?.trim() || row.stem
      await api.commitImport({
        temp_token: p.temp_token,
        title,
        authors: meta?.authors || [],
        year: meta?.year ?? null,
        doi: meta?.doi || '',
        tags: meta?.tags || [],
        project: batchProject.value.trim() || null
      })
      row.state = 'ok'
      okCount++
    } catch (e) {
      row.state = 'fail'
      row.error = String((e as Error).message || e)
      fails.push(`${row.stem}：${row.error}`)
    }
    batchProgress.value.done++
  }
  batchImporting.value = false
  batchDone.value = true
  if (okCount > 0) {
    toastOk(`批量导入完成：成功 ${okCount} 篇`)
    await store.refresh()
    emit('imported')
  }
  if (fails.length) {
    ElMessage.error(`部分失败（${fails.length} 篇）：${fails.slice(0, 3).join('；')}${fails.length > 3 ? '…' : ''}`)
  }
}

const DUP_REASON: Record<string, string> = { sha256: '相同 PDF 内容', doi: '相同 DOI', title: '相似标题' }
</script>

<template>
  <el-dialog
    v-model="visible"
    title="导入文献"
    width="min(1060px, 92vw)"
    top="5vh"
    :close-on-click-modal="false"
    destroy-on-close
    class="import-dialog"
  >
    <div class="dialog-toolbar">
      <el-radio-group v-model="activeTab">
        <el-radio-button value="single">单篇导入</el-radio-button>
        <el-radio-button value="batch">批量导入</el-radio-button>
      </el-radio-group>
      <el-button text :icon="Download" @click="downloadTemplate">下载 AI 分析模板</el-button>
    </div>

    <!-- ============ 单篇 ============ -->
    <div v-if="activeTab === 'single'">
      <el-steps :active="step" align-center finish-status="success" class="steps">
        <el-step title="选择文件" description="PDF 论文与 AI 分析 Markdown" />
        <el-step title="核对信息" description="确认配对与元数据" />
        <el-step title="完成" description="进入阅读" />
      </el-steps>

      <template v-if="step === 0">
        <div class="pick-grid">
          <div class="pick-box" :class="{ filled: pdfPath }" @click="pickPdf">
            <el-icon :size="28" color="var(--el-color-primary)"><Document /></el-icon>
            <div class="pick-title">{{ pdfPath || '选择 PDF 论文' }}</div>
            <div class="pick-sub">{{ pdfPath ? '点击重新选择' : '也可以稍后补 MD，先只导入 PDF' }}</div>
          </div>
          <div class="pick-box" :class="{ filled: mdPath }" @click="pickMd">
            <el-icon :size="28" color="#67c23a"><Promotion /></el-icon>
            <div class="pick-title">{{ mdPath || '选择分析 Markdown' }}</div>
            <div class="pick-sub">{{ mdPath ? '点击重新选择' : '模板格式：YAML 元数据 + 八个固定栏目' }}</div>
          </div>
        </div>

        <div class="paste-toggle">
          <el-link type="primary" @click="showPaste = !showPaste">
            {{ showPaste ? '收起粘贴框' : '没有 MD 文件？直接粘贴分析内容' }}
          </el-link>
        </div>
        <el-input
          v-if="showPaste"
          v-model="mdPasted"
          type="textarea"
          :rows="6"
          placeholder="粘贴外部 AI 生成的 Markdown 分析（需符合模板：YAML 元数据 + 八个固定二级标题）"
        />

        <div class="actions-row">
          <el-button type="primary" :icon="UploadFilled" :loading="analyzing" :disabled="!canAnalyze" @click="analyze">
            {{ analyzing ? '正在解析…' : '解析预览' }}
          </el-button>
        </div>
      </template>

      <template v-else-if="step === 1 && preview">
        <el-alert v-if="preview.pairing_hint" type="warning" :title="preview.pairing_hint" show-icon :closable="false" class="gap" />
        <el-alert
          v-for="d in preview.duplicates"
          :key="d.paper_id"
          type="warning"
          show-icon
          :closable="false"
          class="gap"
        >
          <template #title>
            疑似重复（{{ DUP_REASON[d.reason] || d.reason }}）：已存在「{{ d.title }}」
            <el-radio-group v-model="form.updatePaperId" size="small" style="margin-left: 12px">
              <el-radio-button :value="d.paper_id">更新该文献</el-radio-button>
              <el-radio-button value="">作为新文献</el-radio-button>
            </el-radio-group>
          </template>
        </el-alert>
        <el-alert
          v-if="form.updatePaperId"
          type="info"
          show-icon
          :closable="false"
          class="gap"
          title="更新已有文献：保留更新前的分析恢复副本与个人笔记"
        />
        <el-alert
          v-if="form.updatePaperId && pdfPath"
          type="warning"
          show-icon
          :closable="false"
          class="gap"
        >
          <el-checkbox v-model="form.replacePdf">同时替换 PDF 文件（原核查状态将全部重置）</el-checkbox>
        </el-alert>
        <el-alert
          v-for="w in [...preview.page_warnings, ...(preview.pdf?.page_count_error ? [preview.pdf.page_count_error] : []), ...(preview.md?.warnings || [])]"
          :key="w"
          type="warning"
          :title="w"
          show-icon
          :closable="false"
          class="gap"
        />

        <el-descriptions :column="3" border size="small" class="gap">
          <el-descriptions-item label="固定栏目">
            <el-tag :type="foundCount === 8 ? 'success' : 'warning'" size="small">{{ foundCount }} / 8</el-tag>
          </el-descriptions-item>
          <el-descriptions-item label="PDF">
            <template v-if="preview.pdf">
              {{ preview.pdf.page_count ? `${preview.pdf.page_count} 页 · ` : '' }}{{ (preview.pdf.size / 1024 / 1024).toFixed(1) }} MB
            </template>
            <span v-else class="dim">未选择</span>
          </el-descriptions-item>
          <el-descriptions-item label="页码链接">{{ preview.md?.page_links.length ?? 0 }} 处</el-descriptions-item>
        </el-descriptions>

        <el-form label-position="top" class="form-grid">
          <el-form-item required>
            <template #label>标题 <span class="req">*</span></template>
            <el-input v-model="form.title" placeholder="正式入库时必填" />
          </el-form-item>
          <el-form-item>
            <template #label>作者（逗号分隔）</template>
            <el-input v-model="form.authors" placeholder="未知可留空" />
          </el-form-item>
          <el-form-item label="年份">
            <el-input-number v-model="form.year" :min="1900" :max="2100" style="width: 100%" />
          </el-form-item>
          <el-form-item label="DOI">
            <el-input v-model="form.doi" />
          </el-form-item>
          <el-form-item label="项目">
            <el-select v-model="form.project" filterable allow-create default-first-option placeholder="选择或输入新项目" style="width: 100%">
              <el-option v-for="p in store.projects" :key="p.id" :label="p.name" :value="p.name" />
            </el-select>
          </el-form-item>
          <el-form-item label="标签">
            <el-select v-model="form.tags" multiple filterable allow-create default-first-option placeholder="输入后回车添加" style="width: 100%">
              <el-option v-for="t in store.tags" :key="t.id" :label="t.name" :value="t.name" />
            </el-select>
          </el-form-item>
        </el-form>

        <el-collapse v-if="preview.md" class="gap">
          <el-collapse-item name="md">
            <template #title>预览解析后的 Markdown</template>
            <div class="md-preview-body">
              <MdRender
                :md="pickedMdText || preview.md.sections.map((s) => `## ${s.title}\n\n${s.content}`).join('\n\n')"
              />
            </div>
          </el-collapse-item>
        </el-collapse>

        <div class="actions-row commit-row">
          <el-button :icon="ArrowLeft" @click="step = 0">上一步</el-button>
          <el-button type="primary" :loading="committing" :disabled="!canCommit" @click="commit">
            {{ form.updatePaperId ? '更新文献' : '导入' }}
          </el-button>
        </div>
      </template>
    </div>

    <!-- ============ 批量 ============ -->
    <div v-else>
      <el-alert
        type="info"
        :closable="false"
        show-icon
        class="gap"
        title="一次选择多个 PDF 与 MD 文件：同名（不含扩展名）自动配对；也可以只放 PDF（稍后补分析）或只放 MD。"
      />
      <div class="batch-picks">
        <el-button :icon="Document" @click="pickBatchPdfs">
          添加 PDF 文件
          <el-tag v-if="batchPdfList.length" size="small" style="margin-left: 6px">{{ batchPdfList.length }}</el-tag>
        </el-button>
        <el-button :icon="Promotion" @click="pickBatchMds">
          添加 MD 文件
          <el-tag v-if="batchMdList.length" size="small" style="margin-left: 6px">{{ batchMdList.length }}</el-tag>
        </el-button>
        <span class="batch-spacer"></span>
        <el-select
          v-model="batchProject"
          filterable
          allow-create
          default-first-option
          placeholder="统一归入项目（可选）"
          clearable
          style="width: 200px"
        >
          <el-option v-for="p in store.projects" :key="p.id" :label="p.name" :value="p.name" />
        </el-select>
      </div>

      <el-table v-if="batchRows.length" :data="batchRows" size="small" max-height="300" class="gap">
        <el-table-column label="文件" min-width="220">
          <template #default="{ row }">
            <div class="file-cell">
              <span v-if="row.pdf" class="file-name" :title="row.pdf.name">{{ row.pdf.name }}</span>
              <span v-if="row.md" class="file-name md" :title="row.md.name">{{ row.md.name }}</span>
            </div>
          </template>
        </el-table-column>
        <el-table-column label="识别标题" min-width="180" show-overflow-tooltip>
          <template #default="{ row }">
            {{ row.preview?.md?.meta?.title || (row.state === 'ok' ? '—' : row.stem) }}
          </template>
        </el-table-column>
        <el-table-column label="配对" width="90">
          <template #default="{ row }">
            <el-tag size="small" :type="rowStatus(row).type" effect="plain">{{ rowStatus(row).text }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="栏目" width="70">
          <template #default="{ row }">
            <span v-if="batchHasAnalysis(row) !== null">{{ batchHasAnalysis(row) }} / 8</span>
            <span v-else class="dim">—</span>
          </template>
        </el-table-column>
        <el-table-column label="重复提示" min-width="140" show-overflow-tooltip>
          <template #default="{ row }">
            <span v-if="row.preview?.duplicates?.length" class="warn-text">
              {{ DUP_REASON[row.preview.duplicates[0].reason] || '疑似重复' }}：{{ row.preview.duplicates[0].title }}
            </span>
            <span v-else-if="row.state === 'fail'" class="warn-text" :title="row.error">{{ row.error }}</span>
            <span v-else class="dim">—</span>
          </template>
        </el-table-column>
        <el-table-column width="60">
          <template #default="{ row }">
            <el-button text type="danger" size="small" :disabled="batchImporting" @click="removeRow(row.key)">移除</el-button>
          </template>
        </el-table-column>
      </el-table>

      <el-empty v-else description="还没有选择文件：点击上方按钮添加" :image-size="70" />

      <div class="actions-row">
        <template v-if="batchImporting">
          <el-progress
            :percentage="batchProgress.total ? Math.round((batchProgress.done / batchProgress.total) * 100) : 0"
            style="max-width: 360px"
          />
          <span class="dim">{{ batchProgress.done }} / {{ batchProgress.total }} 篇</span>
        </template>
        <template v-else>
          <span v-if="batchDone" class="dim">
            完成：成功 {{ batchRows.filter((r) => r.state === 'ok').length }} 篇
            <template v-if="batchRows.some((r) => r.state === 'fail')">
              ，失败 {{ batchRows.filter((r) => r.state === 'fail').length }} 篇
            </template>
          </span>
          <el-button :icon="Files" type="primary" :disabled="!batchReady" @click="importAll">
            导入全部（{{ batchRows.length }} 篇）
          </el-button>
        </template>
      </div>
    </div>
  </el-dialog>
</template>

<style scoped>
.dialog-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.steps {
  margin-bottom: 16px;
}

.pick-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}

.pick-box {
  border: 1.5px dashed #e2e5ea;
  border-radius: 10px;
  padding: 24px 16px;
  text-align: center;
  cursor: pointer;
  transition: border-color 0.2s, background 0.2s, transform 0.15s;
}

.pick-box:hover {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
  transform: translateY(-2px);
}

.pick-box.filled {
  border-style: solid;
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}

.pick-title {
  margin-top: 8px;
  font-weight: 600;
  font-size: 14px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pick-sub {
  margin-top: 4px;
  font-size: 12px;
  color: #6a737d;
}

.paste-toggle {
  margin: 12px 0 8px;
}

.actions-row {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 14px;
  margin-top: 16px;
}

.commit-row {
  justify-content: flex-end;
}

.gap {
  margin-bottom: 10px;
}

.dim {
  color: #6a737d;
  font-size: 12px;
}

.warn-text {
  color: #e6a23c;
  font-size: 12px;
}

.req {
  color: #f56c6c;
}

.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0 20px;
}

.md-preview-body {
  max-height: 300px;
  overflow-y: auto;
  padding: 4px 12px;
  border: 1px solid #e2e5ea;
  border-radius: 6px;
}

.batch-picks {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}

.batch-spacer {
  flex: 1;
}

.file-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.file-name {
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--el-color-primary);
}

.file-name.md {
  color: #67c23a;
}
</style>
