<script setup lang="ts">
// 对比页：2～5 篇文献按固定维度横向比较；来源页码可预览原文；
// 个人综合结论（共识/分歧/研究空白/结论）随对比记录保存。
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { ArrowLeft, Delete, Download, Plus } from '@element-plus/icons-vue'
import { api, type AnalysisResponse, type PaperListItem, type Synthesis } from '../ipc'
import { useLibraryStore } from '../stores/library'
import { confirmAction, toastError, toastInfo, toastOk } from '../lib/toast'
import MdRender from '../components/MdRender.vue'

const route = useRoute()
const router = useRouter()
const store = useLibraryStore()

const ALL_DIMENSIONS = ['研究问题', '研究方法', '样本与数据', '主要发现', '创新点', '局限性', '一句话总结', '阅读重点']
const DEFAULT_DIMENSIONS = ['研究问题', '研究方法', '样本与数据', '主要发现', '创新点', '局限性']

interface PaperSlot {
  paper: PaperListItem
  analysis: AnalysisResponse | null
}

const compareId = ref<string | null>(route.params.id ? String(route.params.id) : null)
const isNew = computed(() => route.path === '/compare/new')
const name = ref('')
const slots = ref<PaperSlot[]>([])
const dimensions = ref<string[]>([...DEFAULT_DIMENSIONS])
const synthesis = ref<Synthesis>({ agreement: '', differences: '', gap: '', conclusion: '' })
const updatedPapers = ref<string[]>([])
const loading = ref(true)
const saving = ref(false)

// 来源预览抽屉（writable computed 驱动 el-drawer 的 v-model）
const drawer = ref<{ paperId: string; title: string; page: number } | null>(null)
const drawerOpen = computed({
  get: () => drawer.value !== null,
  set: (v: boolean) => {
    if (!v) drawer.value = null
  }
})

function gotoRead(paperId: string) {
  router.push(`/read/${paperId}`)
}

// 综合结论折叠状态：默认收起，避免挤压对比表格
const synthOpen = ref(false)

// 添加文献弹窗
const showPicker = ref(false)
const pickerSearch = ref('')
const pickerCandidates = ref<PaperListItem[]>([])
const pickerLoading = ref(false)

async function loadSlot(paperId: string): Promise<PaperSlot | null> {
  try {
    const [paper, analysis] = await Promise.all([api.getPaper(paperId), api.getAnalysis(paperId)])
    return { paper, analysis }
  } catch {
    toastError(`文献 ${paperId} 加载失败`)
    return null
  }
}

async function loadFromIds(ids: string[]) {
  const loaded = await Promise.all(ids.map(loadSlot))
  slots.value = loaded.filter((s): s is PaperSlot => s !== null)
}

onMounted(async () => {
  await store.ensureLoaded().catch(() => undefined)
  try {
    if (isNew.value) {
      const p = String(route.query.p || '')
      const ids = p.split(',').filter(Boolean)
      if (ids.length < 2) {
        toastInfo('请先在文献库中勾选 2～5 篇文献再发起对比')
        router.replace('/')
        return
      }
      await loadFromIds(ids)
      name.value = `对比 ${new Date().toLocaleDateString()}`
    } else if (compareId.value) {
      const rec = await api.getCompare(compareId.value)
      name.value = rec.name
      dimensions.value = rec.dimensions.length ? rec.dimensions : [...DEFAULT_DIMENSIONS]
      synthesis.value = { ...rec.synthesis }
      updatedPapers.value = rec.updated_papers
      await loadFromIds(rec.paper_ids)
    }
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    loading.value = false
  }
})

const canSave = computed(() => slots.value.length >= 2 && Boolean(name.value.trim()) && !saving.value)

function sectionOf(slot: PaperSlot, dim: string): { content: string; missing: boolean } {
  const parsed = slot.analysis?.parsed
  if (!parsed) return { content: '', missing: true }
  const sec = parsed.sections.find((s) => s.title === dim)
  if (!sec || !sec.content.trim()) return { content: '', missing: true }
  return { content: sec.content, missing: false }
}

function cellPlaceholder(slot: PaperSlot): string | null {
  if (!slot.analysis) return '未导入分析'
  if (slot.analysis.parsed.parse_status === 'unparsed') return '格式待整理'
  return null
}

function toggleDim(d: string) {
  const i = dimensions.value.indexOf(d)
  if (i >= 0) dimensions.value.splice(i, 1)
  else dimensions.value = ALL_DIMENSIONS.filter((x) => dimensions.value.includes(x) || x === d)
}

function movePaper(idx: number, dir: -1 | 1) {
  const j = idx + dir
  if (j < 0 || j >= slots.value.length) return
  const arr = slots.value
  ;[arr[idx], arr[j]] = [arr[j], arr[idx]]
}

function removePaper(idx: number) {
  slots.value.splice(idx, 1)
}

async function openPicker() {
  if (slots.value.length >= 5) {
    ElMessage.warning('最多对比 5 篇文献')
    return
  }
  showPicker.value = true
  pickerLoading.value = true
  try {
    const list = await api.listPapers({})
    const inUse = new Set(slots.value.map((s) => s.paper.id))
    pickerCandidates.value = list.filter((p) => !inUse.has(p.id))
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    pickerLoading.value = false
  }
}

const filteredCandidates = computed(() => {
  const kw = pickerSearch.value.trim().toLowerCase()
  if (!kw) return pickerCandidates.value
  return pickerCandidates.value.filter((p) => p.title.toLowerCase().includes(kw))
})

async function addPaper(id: string) {
  const slot = await loadSlot(id)
  if (slot) {
    slots.value.push(slot)
    showPicker.value = false
  }
}

function openSource(paperId: string, title: string, page: number) {
  drawer.value = { paperId, title, page }
}

function onCellGoto(paperId: string, title: string) {
  return (page: number) => openSource(paperId, title, page)
}

async function save() {
  if (!canSave.value) return
  saving.value = true
  try {
    const payload = {
      name: name.value.trim(),
      paper_ids: slots.value.map((s) => s.paper.id),
      dimensions: dimensions.value,
      synthesis: synthesis.value
    }
    if (compareId.value) {
      await api.updateCompare(compareId.value, payload)
      toastOk('对比已保存')
    } else {
      const rec = await api.createCompare(payload)
      compareId.value = rec.id
      toastOk('对比已保存')
      router.replace(`/compare/${rec.id}`)
    }
    updatedPapers.value = []
    await store.refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    saving.value = false
  }
}

async function exportMd() {
  if (!compareId.value) {
    ElMessage.warning('请先保存对比，再导出')
    return
  }
  try {
    const r = await api.exportCompare(compareId.value)
    await api.saveText(r.filename, r.content)
    toastOk('已导出对比综述 Markdown')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function removeCompare() {
  if (!compareId.value) return
  const ok = await confirmAction(
    '删除该对比记录？文献本身不受影响，但应用内将无法再打开此对比。',
    '删除对比',
    '删除'
  )
  if (!ok) return
  try {
    await api.deleteCompare(compareId.value)
    toastOk('对比已删除')
    await store.refresh()
    router.replace('/')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

const SYNTHESIS_FIELDS: { key: keyof Synthesis; label: string; hint: string }[] = [
  { key: 'agreement', label: '共识', hint: '多篇文献一致支持的结论及依据' },
  { key: 'differences', label: '分歧', hint: '结论冲突之处及各自条件（保留原始表述，不直接排出优劣）' },
  { key: 'gap', label: '研究空白', hint: '现有研究未覆盖、值得后续跟进的问题' },
  { key: 'conclusion', label: '个人结论', hint: '你的综合判断与下一步计划' }
]
</script>

<template>
  <div class="compare-page" v-loading="loading" element-loading-text="加载对比数据…">
    <header class="cmp-header">
      <div class="header-left">
        <el-button text :icon="ArrowLeft" @click="router.push('/')">文献库</el-button>
        <el-input v-model="name" class="name-input" placeholder="对比名称" maxlength="60" />
      </div>
      <div class="header-right">
        <el-button :icon="Plus" @click="openPicker">添加文献</el-button>
        <el-button type="primary" :loading="saving" :disabled="!canSave" @click="save">保存</el-button>
        <el-button :icon="Download" @click="exportMd">导出 MD</el-button>
        <el-button v-if="compareId" :icon="Delete" type="danger" plain @click="removeCompare">删除</el-button>
      </div>
    </header>

    <template v-if="!loading">
      <!-- 分析更新提示 -->
      <el-alert
        v-if="updatedPapers.length"
        type="warning"
        show-icon
        :closable="false"
        class="update-banner"
      >
        <template #title>
          以下文献的分析在上次查看后有更新，对比内容已刷新为最新版本，请重新核查：
          <strong>{{
            slots
              .filter((s) => updatedPapers.includes(s.paper.id))
              .map((s) => s.paper.title)
              .join('；')
          }}</strong>
        </template>
      </el-alert>

      <!-- 维度选择 -->
      <div class="dim-row">
        <span class="dim-label">对比维度</span>
        <el-check-tag
          v-for="d in ALL_DIMENSIONS"
          :key="d"
          :checked="dimensions.includes(d)"
          @change="toggleDim(d)"
        >{{ d }}</el-check-tag>
      </div>

      <!-- 对比表 -->
      <div class="table-wrap">
        <table v-if="slots.length" class="cmp-table">
          <thead>
            <tr>
              <th class="dim-col">维度</th>
              <th v-for="(s, i) in slots" :key="s.paper.id" class="paper-col">
                <div class="paper-col-head">
                  <el-tooltip :content="s.paper.title" placement="top" :show-after="500">
                    <span class="paper-col-title">{{ s.paper.title }}</span>
                  </el-tooltip>
                  <span class="paper-col-ops">
                    <el-button text size="small" :disabled="i === 0" @click="movePaper(i, -1)">←</el-button>
                    <el-button text size="small" :disabled="i === slots.length - 1" @click="movePaper(i, 1)">→</el-button>
                    <el-button text size="small" type="danger" @click="removePaper(i)">✕</el-button>
                  </span>
                </div>
                <div class="paper-col-meta">
                  {{ s.paper.year || '—' }} ·
                  {{ s.paper.analysis_status === 'comparable' ? '可对比' : s.paper.analysis_status === 'needs_formatting' ? '格式待整理' : '未导入' }}
                </div>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="dim in dimensions" :key="dim">
              <td class="dim-col dim-name">{{ dim }}</td>
              <td v-for="s in slots" :key="s.paper.id" class="cell">
                <div v-if="cellPlaceholder(s)" class="cell-placeholder">
                  <el-tag type="info" effect="plain" size="small">{{ cellPlaceholder(s) }}</el-tag>
                </div>
                <div v-else-if="sectionOf(s, dim).missing" class="cell-placeholder">
                  <el-tag type="warning" effect="plain" size="small">待补充</el-tag>
                </div>
                <MdRender
                  v-else
                  :md="sectionOf(s, dim).content"
                  @goto-page="onCellGoto(s.paper.id, s.paper.title)"
                />
              </td>
            </tr>
          </tbody>
        </table>
        <el-empty v-else description="没有可对比的文献" />
      </div>

      <!-- 综合结论（可折叠，带动画） -->
      <div class="synthesis-bar">
        <span class="synthesis-bar-title">我的综合结论</span>
        <el-tag v-if="synthesis.conclusion" type="success" size="small" effect="plain">已填写个人结论</el-tag>
        <span class="spacer"></span>
      </div>
      <el-collapse-transition>
        <section v-show="synthOpen" class="synthesis-panel">
          <p class="hint">保存在对比记录中，不会被文献分析的更新改写；导出时随综述一并输出。</p>
          <div v-for="f in SYNTHESIS_FIELDS" :key="f.key" class="synth-field">
            <div class="synth-label">
              {{ f.label }}
              <span class="synth-hint">{{ f.hint }}</span>
            </div>
            <el-input
              v-model="synthesis[f.key]"
              type="textarea"
              :autosize="{ minRows: 2, maxRows: 8 }"
              :placeholder="`记录${f.label}…`"
            />
          </div>
        </section>
      </el-collapse-transition>
      <div class="synthesis-toggle">
        <el-button text @click="synthOpen = !synthOpen">
          {{ synthOpen ? '收起综合结论 ▲' : '展开编辑综合结论 ▼' }}
        </el-button>
      </div>
    </template>

    <!-- 添加文献弹窗 -->
    <el-dialog v-model="showPicker" title="添加文献" width="600px">
      <div class="picker-tools">
        <el-input v-model="pickerSearch" placeholder="搜索标题筛选" clearable />
        <span class="picker-count">还可添加 {{ 5 - slots.length }} 篇</span>
      </div>
      <div v-loading="pickerLoading" class="picker-list">
        <el-empty v-if="!pickerLoading && filteredCandidates.length === 0" description="没有可添加的文献" :image-size="60" />
        <div
          v-for="p in filteredCandidates"
          :key="p.id"
          class="picker-row"
          @click="addPaper(p.id)"
        >
          <span class="picker-title">{{ p.title }}</span>
          <span class="picker-meta">
            {{ p.year || '—' }} ·
            <el-tag size="small" :type="p.analysis_status === 'comparable' ? 'success' : 'info'" effect="plain">
              {{ p.analysis_status === 'comparable' ? '可对比' : '无结构化分析' }}
            </el-tag>
          </span>
          <el-icon class="picker-add"><Plus /></el-icon>
        </div>
      </div>
    </el-dialog>

    <!-- 来源预览抽屉 -->
    <el-drawer
      v-model="drawerOpen"
      :title="drawer ? `${drawer.title} · 第 ${drawer.page} 页` : '来源预览'"
      size="640px"
      destroy-on-close
    >
      <div v-if="drawer" class="drawer-body">
        <div class="drawer-ops">
          <el-button type="primary" size="small" @click="gotoRead(drawer.paperId)">
            在阅读页打开并跳到第 {{ drawer.page }} 页
          </el-button>
        </div>
        <iframe
          v-if="!api.isTauri"
          class="drawer-frame"
          :src="`${api.getPdfUrl(drawer.paperId)}#page=${drawer.page}`"
        ></iframe>
        <el-alert
          v-else
          type="info"
          :closable="false"
          title="桌面版请在阅读页查看 PDF 原文（点击上方按钮跳转）"
        />
      </div>
    </el-drawer>
  </div>
</template>

<style scoped>
.compare-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.cmp-header {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 8px 16px;
  background: #fff;
  border-bottom: 1px solid var(--border);
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
  flex: 1;
  min-width: 0;
}

.name-input {
  font-weight: 600;
  max-width: 420px;
}

.name-input :deep(input) {
  font-size: 15px;
  font-weight: 600;
}

.header-right {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
}

.update-banner {
  margin: 12px 16px 0;
}

/* 维度选择 */
.dim-row {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 10px 16px;
}

.dim-label {
  font-size: 12px;
  color: var(--text-dim);
  font-weight: 600;
}

/* 对比表 */
.table-wrap {
  flex: 1;
  overflow: auto;
  padding: 0 16px;
}

.cmp-table {
  border-collapse: separate;
  border-spacing: 0;
  min-width: 100%;
}

.dim-col {
  position: sticky;
  left: 0;
  z-index: 2;
  background: #fff;
  border: 1px solid var(--border);
  width: 104px;
  min-width: 104px;
}

.dim-name {
  font-weight: 600;
  background: #f7f8fa;
  padding: 8px 10px;
}

.paper-col {
  border: 1px solid var(--border);
  min-width: 280px;
  vertical-align: top;
  background: #fff;
}

.paper-col-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px 2px;
}

.paper-col-title {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.paper-col-ops {
  flex: none;
  display: flex;
  align-items: center;
}

.paper-col-ops .el-button {
  padding: 2px 6px;
}

.paper-col-meta {
  padding: 0 10px 6px;
  font-size: 12px;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border);
}

.cell {
  border: 1px solid var(--border);
  vertical-align: top;
  padding: 8px 12px;
  font-size: 13px;
  min-width: 280px;
  background: #fff;
}

.cell-placeholder {
  padding: 4px 0;
}

/* 综合结论 */
.synthesis-bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 20px 0;
}

.synthesis-bar-title {
  font-size: 15px;
  font-weight: 600;
}

.spacer {
  flex: 1;
}

.synthesis-panel {
  flex: none;
  background: #fff;
  border-top: 1px solid var(--border);
  margin: 6px 16px 0;
  padding: 10px 16px;
  border-radius: 8px;
  border: 1px solid var(--border);
  max-height: 40vh;
  overflow-y: auto;
}

.hint {
  color: var(--text-dim);
  font-size: 12px;
  margin: 0 0 10px;
}

.synth-field {
  margin-bottom: 10px;
}

.synth-label {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 4px;
}

.synth-hint {
  font-weight: 400;
  font-size: 12px;
  color: var(--text-dim);
}

.synthesis-toggle {
  flex: none;
  display: flex;
  justify-content: center;
  padding: 2px 0 8px;
}

/* 弹窗与抽屉 */
.picker-tools {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 10px;
}

.picker-tools .el-input {
  flex: 1;
}

.picker-count {
  flex: none;
  font-size: 12px;
  color: var(--text-dim);
}

.picker-list {
  min-height: 200px;
  max-height: 380px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.picker-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}

.picker-row:hover {
  border-color: var(--primary);
  background: var(--el-color-primary-light-9);
}

.picker-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
}

.picker-meta {
  flex: none;
  font-size: 12px;
  color: var(--text-dim);
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.picker-add {
  color: var(--primary);
}

.drawer-body {
  display: flex;
  flex-direction: column;
  height: 100%;
  gap: 10px;
}

.drawer-ops {
  flex: none;
}

.drawer-frame {
  flex: 1;
  border: 1px solid var(--border);
  border-radius: 6px;
  width: 100%;
}
</style>
