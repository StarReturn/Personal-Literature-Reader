<script setup lang="ts">
// 阅读页：PDF 阅读（翻页/跳转/缩放/位置记忆）+ 批注（选中高亮/矩形框/颜色/备注）
// + MD 分析 + 个人笔记，三种布局可切换。
// 批注坐标为页面归一化值（0-1），与缩放无关；数据存 SQLite，PDF 原文件永不修改。
import { computed, markRaw, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { ArrowLeft, Crop, Download, Edit, Hide, Memo, Minus, Plus, RefreshRight, Select as SelectTool } from '@element-plus/icons-vue'
import { api, type AnalysisResponse, type EvidenceRecord, type PaperDetail, type PdfAnnotation } from '../ipc'
import { confirmAction, toastError, toastInfo, toastOk } from '../lib/toast'
import { anchorHash, stripFrontMatter } from '../lib/markdown'
import { addToBasket } from '../lib/basket'
import MdRender from '../components/MdRender.vue'
import MarkdownEditor from '../components/MarkdownEditor.vue'
import PdfReader from '../components/pdf/PdfReader.vue'

const route = useRoute()
const router = useRouter()
const paperId = route.params.id as string

const paper = ref<PaperDetail | null>(null)
const analysis = ref<AnalysisResponse | null>(null)
const evidence = ref<EvidenceRecord[]>([])
const noteText = ref('')
const noteSavedAt = ref(0)
const noteDirty = ref(false)

type Mode = 'pdf' | 'md' | 'split'
const mode = ref<Mode>('split')
const rightTab = ref<'analysis' | 'notes' | 'edit'>('analysis')

// ---------- PDF 状态 ----------
import * as pdfjsLib from 'pdfjs-dist'
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.js?url'
pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl

// pdf.js 文档对象含 ES 私有字段，禁止深层响应式代理，用 shallowRef + markRaw 保存
const pdfDoc = shallowRef<pdfjsLib.PDFDocumentProxy | null>(null)
const pdfLoading = ref(true)
const pdfError = ref('')
const pageNum = ref(1)
const zoom = ref(1.0)
const canvasRef = ref<HTMLCanvasElement | null>(null)
const textLayerRef = ref<HTMLDivElement | null>(null)
const pdfStageRef = ref<HTMLDivElement | null>(null)
const pdfScrollRef = ref<HTMLDivElement | null>(null)
const rendering = ref(false)
let renderQueued = false
let scrollAfterPageChange: 'top' | 'bottom' | null = null
let nextWheelFlipAt = 0
let touchStart: { y: number; atTop: boolean; atBottom: boolean } | null = null
const canvasW = ref(0)
const canvasH = ref(0)

const pageCount = computed(() => pdfDoc.value?.numPages || paper.value?.pdf_page_count || 0)

// ---------- 批注状态 ----------
const ANNO_COLORS = ['yellow', 'red', 'blue', 'green'] as const
const COLOR_HEX: Record<string, string> = {
  yellow: '#f5d76e',
  red: '#f28b82',
  blue: '#8ab4f8',
  green: '#81c995'
}
const COLOR_TITLE: Record<string, string> = { yellow: '黄 · 重点', red: '红 · 质疑', blue: '蓝 · 同意', green: '绿 · 待查' }
const KIND_LABEL: Record<string, string> = { highlight: '高亮', rect: '框选', note: '便签' }

const annoTool = ref<'off' | 'select' | 'rect'>('off')
const annoColor = ref<(typeof ANNO_COLORS)[number]>('yellow')
const annotations = ref<PdfAnnotation[]>([])
const drawing = ref<{ x: number; y: number; w: number; h: number } | null>(null)
const showAnnoList = ref(false)

// 悬停信息卡：显示在鼠标旁（备注 + 原文摘录）
const hoverTip = ref<{ x: number; y: number; anno: PdfAnnotation } | null>(null)

function showTip(a: PdfAnnotation, e: MouseEvent) {
  const stage = pdfStageRef.value
  if (!stage) return
  const r = stage.getBoundingClientRect()
  hoverTip.value = { x: e.clientX - r.left + 12, y: e.clientY - r.top + 12, anno: a }
}

function pinPos(a: PdfAnnotation): { left: string; top: string } {
  const r0 = a.rects[0] || { x: 0.5, y: 0.5 }
  const left = Math.min((r0.x + r0.w) * canvasW.value, canvasW.value - 12)
  const top = Math.max((r0.y) * canvasH.value - 6, 2)
  return { left: `${left}px`, top: `${top}px` }
}

// 编辑批注弹窗（writable computed 桥接 el-dialog v-model）
const editing = ref<PdfAnnotation | null>(null)
const editText = ref('')
const editColor = ref<string>('yellow')
const editTags = ref('')
const editSaving = ref(false)
const editingOpen = computed({
  get: () => editing.value !== null,
  set: (v: boolean) => {
    if (!v) editing.value = null
  }
})

async function loadAnnotations() {
  try {
    annotations.value = await api.listAnnotations(paperId)
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

const pageAnnotations = computed(() => annotations.value.filter((a) => a.page === pageNum.value))

function openEdit(a: PdfAnnotation) {
  editing.value = a
  editText.value = a.text
  editColor.value = a.color
  editTags.value = (a.tags || []).join(', ')
}

async function saveEdit() {
  if (!editing.value || editSaving.value) return
  editSaving.value = true
  try {
    const updated = await api.updateAnnotation(editing.value.id, {
      color: editColor.value,
      text: editText.value,
      tags: editTags.value.split(/[,，]/).map((x) => x.trim()).filter(Boolean)
    })
    const i = annotations.value.findIndex((x) => x.id === updated.id)
    if (i >= 0) annotations.value.splice(i, 1, updated)
    toastOk('批注已更新')
    editing.value = null
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    editSaving.value = false
  }
}

async function removeEdit() {
  if (!editing.value) return
  const ok = await confirmAction('删除这条批注？（不影响 PDF 原文与其他批注）', '删除批注', '删除')
  if (!ok) return
  try {
    await api.deleteAnnotation(editing.value.id)
    annotations.value = annotations.value.filter((x) => x.id !== editing.value!.id)
    editing.value = null
    toastOk('批注已删除')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

/** 选择工具下：鼠标抬起时若在文字层有选区，则保存为高亮批注。 */
async function captureSelection() {
  if (annoTool.value !== 'select') return
  const sel = window.getSelection()
  const layer = textLayerRef.value
  const canvas = canvasRef.value
  if (!sel || sel.isCollapsed || !sel.rangeCount || !layer || !canvas) return
  const anchor = sel.anchorNode
  if (!anchor || !layer.contains(anchor)) return
  const canvasRect = canvas.getBoundingClientRect()
  const rects: { x: number; y: number; w: number; h: number }[] = []
  const range = sel.getRangeAt(0)
  for (const r of Array.from(range.getClientRects())) {
    if (r.width < 1 || r.height < 1) continue
    const x = (r.left - canvasRect.left) / canvasRect.width
    const y = (r.top - canvasRect.top) / canvasRect.height
    if (x < -0.02 || x > 1.02 || y < -0.02 || y > 1.02) continue
    const w = Math.min(r.width / canvasRect.width, 1 - Math.max(0, x))
    const h = Math.min(r.height / canvasRect.height, 1 - Math.max(0, y))
    rects.push({ x: Math.max(0, x), y: Math.max(0, y), w, h })
  }
  const quote = sel.toString().replace(/\s+/g, ' ').trim()
  sel.removeAllRanges()
  if (!rects.length || !quote) return
  try {
    await api.addAnnotation(paperId, {
      page: pageNum.value,
      kind: 'highlight',
      rects,
      color: annoColor.value,
      quote
    })
    await loadAnnotations()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

/** 矩形工具：拖拽画框，抬起保存。 */
function onAnnoLayerMouseDown(e: MouseEvent) {
  if (annoTool.value !== 'rect' || !canvasRef.value) return
  const rect = canvasRef.value.getBoundingClientRect()
  const sx = (e.clientX - rect.left) / rect.width
  const sy = (e.clientY - rect.top) / rect.height
  drawing.value = { x: sx, y: sy, w: 0, h: 0 }
  e.preventDefault()
  const move = (ev: MouseEvent) => {
    const cx = Math.max(0, Math.min(1, (ev.clientX - rect.left) / rect.width))
    const cy = Math.max(0, Math.min(1, (ev.clientY - rect.top) / rect.height))
    drawing.value = {
      x: Math.min(sx, cx),
      y: Math.min(sy, cy),
      w: Math.abs(cx - sx),
      h: Math.abs(cy - sy)
    }
  }
  const up = async () => {
    window.removeEventListener('mousemove', move)
    window.removeEventListener('mouseup', up)
    const d = drawing.value
    drawing.value = null
    if (!d || d.w < 0.008 || d.h < 0.008) return
    try {
      await api.addAnnotation(paperId, {
        page: pageNum.value,
        kind: 'rect',
        rects: [d],
        color: annoColor.value
      })
      await loadAnnotations()
    } catch (err) {
      toastError(String((err as Error).message || err))
    }
  }
  window.addEventListener('mousemove', move)
  window.addEventListener('mouseup', up)
}

async function loadPaper() {
  try {
    paper.value = await api.getPaper(paperId)
    mode.value = (['pdf', 'md', 'split'].includes(paper.value.last_mode) ? paper.value.last_mode : 'split') as Mode
    pageNum.value = Math.max(1, paper.value.last_page || 1)
    zoom.value = paper.value.last_zoom || 1.0
    const [a, note, ev] = await Promise.all([
      api.getAnalysis(paperId),
      api.getNote(paperId),
      api.listEvidence(paperId)
    ])
    analysis.value = a
    noteText.value = note.content
    noteSavedAt.value = note.updated_at
    evidence.value = ev
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function loadPdf() {
  pdfLoading.value = true
  pdfError.value = ''
  try {
    const bytes = await api.getPdfBytes(paperId)
    const task = pdfjsLib.getDocument({ data: bytes })
    pdfDoc.value = markRaw(await task.promise)
    if (pageNum.value > pdfDoc.value.numPages) pageNum.value = 1
  } catch (e) {
    const err = e as Error
    pdfError.value = err.message || String(err)
  } finally {
    pdfLoading.value = false
  }
}

async function renderPage() {
  const doc = pdfDoc.value
  const canvas = canvasRef.value
  if (!doc || !canvas) return
  if (rendering.value) {
    renderQueued = true
    return
  }
  rendering.value = true
  const renderedPage = pageNum.value
  const renderedZoom = zoom.value
  try {
    const page = await doc.getPage(Math.min(renderedPage, doc.numPages))
    // 高 DPI 清晰渲染：canvas 按物理像素绘制（CSS 尺寸不变，位图放大 DPR 倍），
    // 否则在 125%/150% 缩放屏上位图被拉伸发虚
    const dpr = Math.min(window.devicePixelRatio || 1, 2.5)
    const cssViewport = page.getViewport({ scale: renderedZoom })
    const renderViewport = page.getViewport({ scale: renderedZoom * dpr })
    const cssW = Math.floor(cssViewport.width)
    const cssH = Math.floor(cssViewport.height)
    canvas.width = Math.floor(renderViewport.width)
    canvas.height = Math.floor(renderViewport.height)
    canvas.style.width = `${cssW}px`
    canvas.style.height = `${cssH}px`
    canvasW.value = cssW
    canvasH.value = cssH
    await page.render({
      canvasContext: canvas.getContext('2d')!,
      viewport: renderViewport
    }).promise
    await renderTextLayerNow(page, cssViewport)
    if (renderedPage === pageNum.value && renderedZoom === zoom.value && scrollAfterPageChange) {
      const scroll = pdfScrollRef.value
      if (scroll) scroll.scrollTop = scrollAfterPageChange === 'bottom' ? scroll.scrollHeight : 0
      scrollAfterPageChange = null
    }
  } finally {
    rendering.value = false
    if (renderQueued || renderedPage !== pageNum.value || renderedZoom !== zoom.value) {
      renderQueued = false
      void renderPage()
    }
  }
}

/** 渲染透明文字层：支持选中文字并转为高亮批注。 */
async function renderTextLayerNow(page: pdfjsLib.PDFPageProxy, viewport: { width: number; height: number }) {
  const layer = textLayerRef.value
  if (!layer) return
  layer.innerHTML = ''
  layer.style.width = `${canvasW.value}px`
  layer.style.height = `${canvasH.value}px`
  // pdf.js 3.x 文字层依赖 --scale-factor 计算字号与定位，缺失会导致层尺寸无效、无法选中
  layer.style.setProperty('--scale-factor', String(zoom.value))
  try {
    const tc = await page.getTextContent()
    const task = pdfjsLib.renderTextLayer({
      textContentSource: tc,
      container: layer,
      viewport,
      textDivs: []
    })
    await task.promise
  } catch {
    // 扫描版 PDF 无文字层：仅无法选中高亮，框选批注仍可用
  }
}

// ---- PdfReader 桥接 ----
const readerRef = ref<InstanceType<typeof PdfReader> | null>(null)
// 最近一次拖选的页码（PdfReader dragstart 写入 dataTransfer 'app/page'）
const lastQuotePage = ref<number | null>(null)
const readerMode = ref<'vertical' | 'paged'>('vertical')

function gotoPage(p: number) {
  if (!pdfDoc.value) {
    mode.value = 'pdf'
    toastInfo('PDF 未加载完成，稍后再试')
    return
  }
  if (mode.value === 'md') mode.value = 'split'
  nextTick(() => readerRef.value?.gotoPage(p))
}

function onReaderPage(p: number) {
  pageNum.value = p
  if (paper.value && paper.value.reading_status === 'to_read') {
    paper.value.reading_status = 'reading'
    api.patchPaper(paperId, { reading_status: 'reading' }).catch(() => undefined)
  }
}

function onReaderMode(m: 'vertical' | 'paged') {
  readerMode.value = m
}

function flipFromScroll(direction: 1 | -1) {
  if (!pdfDoc.value || rendering.value || pdfLoading.value) return false
  const target = pageNum.value + direction
  if (target < 1 || target > pdfDoc.value.numPages) return false
  scrollAfterPageChange = direction === 1 ? 'top' : 'bottom'
  pageNum.value = target
  return true
}

function onPdfWheel(event: WheelEvent) {
  if (event.ctrlKey || !event.deltaY) return
  const scroll = pdfScrollRef.value
  if (!scroll) return
  const atTop = scroll.scrollTop <= 2
  const atBottom = scroll.scrollTop + scroll.clientHeight >= scroll.scrollHeight - 2
  if (!(event.deltaY > 0 ? atBottom : atTop)) return
  if (performance.now() < nextWheelFlipAt) {
    event.preventDefault()
    return
  }
  if (flipFromScroll(event.deltaY > 0 ? 1 : -1)) {
    event.preventDefault()
    nextWheelFlipAt = performance.now() + 500
  }
}

function onPdfTouchStart(event: TouchEvent) {
  const scroll = pdfScrollRef.value
  if (!scroll || event.touches.length !== 1) return
  touchStart = {
    y: event.touches[0].clientY,
    atTop: scroll.scrollTop <= 2,
    atBottom: scroll.scrollTop + scroll.clientHeight >= scroll.scrollHeight - 2
  }
}

function onPdfTouchEnd(event: TouchEvent) {
  if (!touchStart || event.changedTouches.length !== 1) return
  const distance = event.changedTouches[0].clientY - touchStart.y
  if (distance < -45 && touchStart.atBottom) flipFromScroll(1)
  if (distance > 45 && touchStart.atTop) flipFromScroll(-1)
  touchStart = null
}

watch(pageNum, () => {
  scrollAfterPageChange ??= 'top'
  renderPage()
})
watch(zoom, renderPage)

watch(pageNum, (p) => {
  if (paper.value && paper.value.reading_status === 'to_read') {
    paper.value.reading_status = 'reading'
    api.patchPaper(paperId, { reading_status: 'reading' }).catch(() => undefined)
  }
})

// 阅读位置记忆：防抖保存
let saveTimer: ReturnType<typeof setTimeout> | null = null
function scheduleSavePosition() {
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    api
      .patchPaper(paperId, {
        last_page: pageNum.value,
        last_zoom: zoom.value,
        last_mode: mode.value
      })
      .catch(() => undefined)
  }, 800)
}
watch([pageNum, zoom, mode], scheduleSavePosition)

onBeforeUnmount(() => {
  if (saveTimer) clearTimeout(saveTimer)
  api
    .patchPaper(paperId, { last_page: pageNum.value, last_zoom: zoom.value, last_mode: mode.value })
    .catch(() => undefined)
})

// ---------- 笔记自动保存 ----------
let noteTimer: ReturnType<typeof setTimeout> | null = null
watch(noteText, () => {
  noteDirty.value = true
  if (noteTimer) clearTimeout(noteTimer)
  noteTimer = setTimeout(saveNote, 1200)
})

async function saveNote() {
  try {
    const r = await api.setNote(paperId, noteText.value)
    noteSavedAt.value = r.updated_at
    noteDirty.value = false
  } catch (e) {
    toastError(`笔记保存失败：${String((e as Error).message || e)}`)
  }
}

onBeforeUnmount(() => {
  if (noteTimer) clearTimeout(noteTimer)
  if (noteDirty.value) saveNote()
})

// ---------- MD 编辑 ----------
const editBuffer = ref('')
watch(rightTab, (t) => {
  if (t === 'edit') editBuffer.value = analysis.value?.md_content || ''
})

/** 从本地选择 MD 文件导入编辑区（可预览微调后保存）。 */
async function importMdFile() {
  try {
    const text = await api.pickMdText()
    if (text === null || text === undefined) return
    if (!text.trim()) {
      toastError('所选文件内容为空')
      return
    }
    editBuffer.value = text
    toastOk('已导入 MD 文件内容，确认无误后点「保存分析」')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

/** 编辑区一键填入模板骨架（title 预填当前文献标题）。 */
function fillTemplate() {
  const title = paper.value?.title || ''
  editBuffer.value = `---
schema_version: 1
title: "${title.replace(/"/g, '\\"')}"
authors: []
year: null
doi: ""
source_pdf: ""
tags: []
---

## 一句话总结

## 研究问题

## 研究方法

## 样本与数据

## 主要发现

## 创新点

## 局限性

## 阅读重点
`
}

async function saveMd() {
  if (!editBuffer.value.trim()) {
    toastError('分析内容为空：如需删除分析请使用重新导入')
    return
  }
  try {
    analysis.value = await api.setAnalysis(paperId, editBuffer.value)
    toastOk('分析已保存')
    rightTab.value = 'analysis'
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function restorePrev() {
  const ok = await confirmAction(
    '恢复上一次更新前的分析内容？当前内容将被替换（其副本会继续保留在数据库中）。',
    '恢复上一版'
  )
  if (!ok) return
  try {
    analysis.value = await api.restorePrevAnalysis(paperId)
    toastOk('已恢复上一次的分析')
    rightTab.value = 'analysis'
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

// ---------- 证据核查 ----------
const linkRows = computed(() => {
  const parsed = analysis.value?.parsed
  if (!parsed) return []
  return parsed.page_links.map((l) => ({ page: l.page, context: l.context }))
})

function statusOfLink(page: number, context: string): EvidenceRecord | undefined {
  return evidence.value.find(
    (e) => e.page === page && e.excerpt.slice(0, 60) === context.slice(0, 60)
  )
}

async function toggleCheck(context: string, page: number) {
  const hash = await anchorHash(paperId, `link:${page}`, context)
  const existing = statusOfLink(context, page)
  const next = existing?.check_status === 'verified' ? 'unverified' : 'verified'
  try {
    const rec = await api.upsertEvidence(paperId, {
      anchor_hash: hash,
      section: '',
      excerpt: context,
      page,
      check_status: next
    })
    const i = evidence.value.findIndex((e) => e.anchor_hash === hash)
    if (i >= 0) evidence.value.splice(i, 1, rec)
    else evidence.value.unshift(rec)
    toastOk(next === 'verified' ? '已标记为已核查' : '已重置为待核查')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

// ---------- 顶部操作 ----------
async function setReadingStatus(s: string) {
  if (!paper.value) return
  paper.value.reading_status = s as PaperDetail['reading_status']
  try {
    await api.patchPaper(paperId, { reading_status: s })
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

/** PDF 拖选文字入笔记：生成引用块（题录 + 页码 + 原文）。 */
function onNoteDrop(e: DragEvent) {
  const text = e.dataTransfer?.getData('text/plain')?.trim()
  if (!text) return
  const page = Number(e.dataTransfer?.getData('app/page') || 0)
  lastQuotePage.value = page || null
  const paperTitle = paper.value?.title || '未知文献'
  const cite = `> ${text}\n\n—— ${paperTitle}${lastQuotePage.value ? `（PDF 第 ${lastQuotePage.value} 页）` : ''}\n\n`
  noteText.value = (noteText.value ? noteText.value + '\n' : '') + cite
  noteDirty.value = true
  toastOk('已作为引用块加入笔记')
}

function addToCompare() {
  const r = addToBasket(paperId)
  if (!r.added) {
    ElMessage.warning(r.size >= 5 ? '选择篮已满（最多 5 篇）' : '已在对比选择篮中')
    return
  }
  toastOk(`已加入对比选择（${r.size} 篇），回文献库可发起对比`)
}

async function exportThis() {
  try {
    const r = await api.exportPaper(paperId, true)
    await api.saveText(r.filename, r.content)
    toastOk('已导出 Markdown')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

// ---------- 分栏拖拽 ----------
const splitRatio = ref(0.6)
const splitContainer = ref<HTMLElement | null>(null)
let dragging = false

function onDividerDown(e: MouseEvent) {
  dragging = true
  e.preventDefault()
  const move = (ev: MouseEvent) => {
    if (!dragging || !splitContainer.value) return
    const rect = splitContainer.value.getBoundingClientRect()
    const r = (ev.clientX - rect.left) / rect.width
    splitRatio.value = Math.min(0.75, Math.max(0.25, r))
  }
  const up = () => {
    dragging = false
    window.removeEventListener('mousemove', move)
    window.removeEventListener('mouseup', up)
  }
  window.addEventListener('mousemove', move)
  window.addEventListener('mouseup', up)
}

onMounted(async () => {
  // 从文献库「编辑 MD 内容」入口进入时，直达编辑标签（前置设置，避免加载时序问题）
  if (route.query.tab === 'edit' || location.hash.includes('tab=edit')) {
    rightTab.value = 'edit'
  }
  await loadPaper()
  await loadAnnotations()
  if (mode.value !== 'md') await loadPdf()
})

watch(mode, (m) => {
  if (m !== 'md' && !pdfDoc.value) loadPdf()
  else nextTick(() => renderPage())
})
</script>

<template>
  <div class="read-page">
    <!-- 顶栏 -->
    <header class="read-header">
      <div class="header-left">
        <el-button text :icon="ArrowLeft" @click="router.push('/')">文献库</el-button>
        <el-tooltip :content="paper?.title || ''" placement="bottom" :show-after="600">
          <span class="paper-title">{{ paper?.title || '…' }}</span>
        </el-tooltip>
      </div>
      <div class="header-center">
        <el-radio-group v-model="mode" size="small">
          <el-radio-button value="pdf">PDF</el-radio-button>
          <el-radio-button value="md">分析</el-radio-button>
          <el-radio-button value="split">分栏</el-radio-button>
        </el-radio-group>
        <el-select
          v-if="paper"
          :model-value="paper.reading_status"
          size="small"
          style="width: 108px"
          @change="setReadingStatus"
        >
          <el-option label="待阅读" value="to_read" />
          <el-option label="阅读中" value="reading" />
          <el-option label="已完成" value="finished" />
        </el-select>
      </div>
      <div class="header-right">
        <el-button :icon="Download" @click="exportThis">导出</el-button>
        <el-button :icon="Plus" @click="addToCompare">加入对比</el-button>
      </div>
    </header>

    <!-- 主体 -->
    <div ref="splitContainer" class="read-body">
      <!-- PDF 侧（Zotero 式阅读器批次1：连续滚动/翻页 + 懒渲染 + 缩放工具栏） -->
      <section
        v-if="mode !== 'md'"
        class="pdf-pane"
        :style="mode === 'split' ? { flexBasis: `${splitRatio * 100}%` } : { flex: 1 }"
      >
        <PdfReader
          v-if="pdfDoc"
          ref="readerRef"
          :pdf-doc="pdfDoc"
          :paper-id="paperId"
          :annotations="annotations"
          :initial-page="pageNum"
          :initial-zoom="zoom"
          :initial-mode="readerMode"
          @page-change="onReaderPage"
          @zoom-change="(z) => (zoom = z)"
          @mode-change="onReaderMode"
          @edit-annotation="openEdit"
          @annotations-changed="loadAnnotations"
        />
        <div v-else-if="pdfLoading" class="pdf-loading" v-loading="true" element-loading-text="PDF 加载中…" />
        <el-alert
          v-else-if="pdfError"
          type="error"
          :title="`PDF 无法显示：${pdfError}`"
          show-icon
          :closable="false"
          class="pdf-error-box"
        />
      </section>

      <!-- 拖拽分隔条 -->
      <div v-if="mode === 'split'" class="divider" title="拖拽调整分栏宽度" @mousedown="onDividerDown"></div>

      <!-- 右侧 -->
      <section v-if="mode !== 'pdf'" class="right-pane">
        <el-tabs v-model="rightTab" class="right-tabs">
          <el-tab-pane label="AI 分析" name="analysis" />
          <el-tab-pane name="notes">
            <template #label>
              我的笔记<el-badge v-if="noteDirty" is-dot style="margin-left: 4px" />
            </template>
          </el-tab-pane>
          <el-tab-pane label="编辑 MD" name="edit" />
        </el-tabs>
        <div class="right-tools">
          <el-button
            v-if="rightTab === 'analysis' && analysis?.prev_md_content"
            text
            size="small"
            :icon="RefreshRight"
            @click="restorePrev"
          >恢复上一版</el-button>
        </div>

        <!-- 分析视图 -->
        <div v-if="rightTab === 'analysis'" class="right-scroll">
          <template v-if="analysis">
            <el-alert
              v-if="analysis.parsed.warnings.length"
              type="warning"
              show-icon
              :closable="false"
              class="mb"
            >
              <div v-for="w in analysis.parsed.warnings" :key="w">{{ w }}</div>
            </el-alert>
            <MdRender :md="stripFrontMatter(analysis.md_content)" @goto-page="gotoPage" />

            <el-divider content-position="left">来源核查（{{ linkRows.length }} 处页码链接）</el-divider>
            <div v-if="linkRows.length === 0" class="dim-line">
              分析中没有页码链接。核查时可直接使用左侧 PDF 翻页对照。
            </div>
            <div v-for="(row, i) in linkRows" :key="i" class="evidence-row">
              <span class="evidence-context">{{ row.context }}</span>
              <span class="evidence-ops">
                <el-button size="small" @click="gotoPage(row.page)">第 {{ row.page }} 页</el-button>
                <el-button
                  size="small"
                  :type="statusOfLink(row.page, row.context)?.check_status === 'verified' ? 'success' : 'default'"
                  :plain="statusOfLink(row.page, row.context)?.check_status !== 'verified'"
                  @click="toggleCheck(row.context, row.page)"
                >
                  {{ statusOfLink(row.page, row.context)?.check_status === 'verified' ? '已核查 ✓' : '标记已核查' }}
                </el-button>
                <el-tag v-if="statusOfLink(row.page, row.context)?.stale" type="danger" size="small">
                  内容已变更，待复核
                </el-tag>
              </span>
            </div>
          </template>
          <el-empty v-else description="该文献还没有 AI 分析">
            <p class="dim-line">在文献库列表点「导入MD」，或在这里粘贴模板格式的内容后保存。</p>
            <el-button type="primary" :icon="Edit" @click="rightTab = 'edit'">补充 MD 分析</el-button>
          </el-empty>
        </div>

        <!-- 笔记视图 -->
        <div v-else-if="rightTab === 'notes'" class="right-scroll notes-wrap">
          <div class="dim-line">
            个人笔记独立保存，不会被重新导入的 AI 分析覆盖。自动保存中
            <template v-if="noteDirty">…</template>
            <template v-else-if="noteSavedAt">（已保存）</template>
          </div>
          <div
            class="note-drop-zone"
            @dragover.prevent
            @drop.prevent="onNoteDrop"
          >
            <MarkdownEditor v-model="noteText" placeholder="用 Markdown 记录你的研究判断、疑问和结论…（左侧编辑，右侧实时预览；可从 PDF 拖选文字进来）" />
          </div>
        </div>

        <!-- 编辑视图 -->
        <div v-else class="right-scroll edit-wrap">
          <div class="dim-line">
            直接编辑分析 Markdown（保持模板格式：YAML 元数据 + 八个固定二级标题）。保存后对比页将读取最新内容，相关核查状态按内容对应关系刷新。
            <el-link type="primary" :underline="false" style="margin-left: 6px" @click="fillTemplate">
              {{ editBuffer.trim() ? '重置为模板骨架' : '填入模板骨架' }}
            </el-link>
            <el-link type="primary" :underline="false" style="margin-left: 14px" @click="importMdFile">
              导入 MD 文件
            </el-link>
          </div>
          <MarkdownEditor
            v-model="editBuffer"
            mode="tab"
            placeholder="---&#10;schema_version: 1&#10;title: ...&#10;---&#10;&#10;## 一句话总结&#10;..."
          />
          <div class="edit-actions">
            <el-button @click="rightTab = 'analysis'">取消</el-button>
            <el-button type="primary" @click="saveMd">保存分析</el-button>
          </div>
        </div>
      </section>
    </div>

    <!-- 编辑批注弹窗 -->
    <el-dialog v-model="editingOpen" title="批注" width="420px" :close-on-click-modal="false">
      <template v-if="editing">
        <div class="edit-anno-meta">
          <el-tag size="small" effect="plain">{{ KIND_LABEL[editing.kind] || editing.kind }}</el-tag>
          <span class="dim-line" style="margin: 0">第 {{ editing.page }} 页</span>
        </div>
        <div v-if="editing.quote" class="anno-quote">"{{ editing.quote }}"</div>
        <div class="color-swatches" style="margin: 10px 0 12px">
          <span
            v-for="c in ANNO_COLORS"
            :key="c"
            class="swatch big"
            :class="{ active: editColor === c }"
            :style="{ background: COLOR_HEX[c] }"
            @click="editColor = c"
          ></span>
        </div>
        <el-input v-model="editText" type="textarea" :rows="3" placeholder="备注（可选）：写下你对这段内容的想法…" />
        <div style="margin-top: 10px">
          <el-input v-model="editTags" placeholder="注释标签（逗号分隔，可选）：如 存疑 / 待复核 / 关键数据" />
        </div>
        <div style="margin-top: 8px">
          <el-color-picker v-model="editColor" size="small" />
          <span class="dim-line" style="margin-left: 8px">自定义颜色</span>
        </div>
      </template>
      <template #footer>
        <el-button type="danger" plain @click="removeEdit">删除</el-button>
        <el-button @click="editing = null">取消</el-button>
        <el-button type="primary" :loading="editSaving" @click="saveEdit">保存</el-button>
      </template>
    </el-dialog>

    <!-- 批注列表抽屉 -->
    <el-drawer v-model="showAnnoList" title="批注列表" size="380px">
      <el-empty v-if="annotations.length === 0" description="还没有批注：选中 PDF 文字或用框选工具添加" :image-size="70" />
      <div
        v-for="a in annotations"
        :key="a.id"
        class="anno-list-item"
        @click="gotoPage(a.page); showAnnoList = false"
      >
        <span class="anno-dot" :style="{ background: COLOR_HEX[a.color] }"></span>
        <div class="anno-list-main">
          <div class="anno-list-head">
            第 {{ a.page }} 页 · {{ KIND_LABEL[a.kind] || a.kind }}
          </div>
          <div class="anno-list-text">{{ a.text || a.quote || '（无备注）' }}</div>
        </div>
      </div>
    </el-drawer>
  </div>
</template>

<style scoped>
.read-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

/* ---------- 顶栏 ---------- */
.read-header {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 20px;
  background: var(--app-surface);
  border-bottom: 1px solid var(--app-line);
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
  flex: 1;
  min-width: 0;
}

.paper-title {
  font-weight: 600;
  font-size: 16px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.header-center {
  flex: none;
  display: flex;
  align-items: center;
  gap: 12px;
}

.header-right {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
}

/* ---------- 主体 ---------- */
.read-body {
  flex: 1;
  display: flex;
  min-height: 0;
  overflow: hidden;
}

.pdf-pane {
  flex: none;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.divider {
  flex: none;
  width: 4px;
  cursor: col-resize;
  background: var(--app-line);
  transition: background 0.15s;
}

.divider:hover {
  background: var(--el-color-primary);
}

.pdf-toolbar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--app-line);
  background: var(--app-surface);
  overflow-x: auto;
  white-space: nowrap;
}

.pdf-toolbar > :not(.toolbar-spacer),
.pdf-toolbar :deep(.el-button-group),
.pdf-toolbar :deep(.el-button) {
  flex: none;
}

.pdf-toolbar :deep(.el-button-group) {
  display: inline-flex;
}

.flip-h :deep(.el-icon) {
  transform: scaleX(-1);
}

.toolbar-spacer {
  flex: 1;
}

.page-indicator {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-variant-numeric: tabular-nums;
  color: var(--app-muted);
  font-size: 13px;
}

.page-input {
  width: 56px;
}

.zoom-label {
  min-width: 56px;
  pointer-events: none;
  font-variant-numeric: tabular-nums;
}

/* 颜色选择 */
.color-swatches {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 0 2px;
}

.swatch {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  cursor: pointer;
  border: 2px solid #fff;
  box-shadow: 0 0 0 1px #c8ccd4;
  transition: transform 0.12s;
}

.swatch:hover {
  transform: scale(1.15);
}

.swatch.active {
  box-shadow: 0 0 0 2px var(--el-color-primary);
}

.swatch.big {
  width: 22px;
  height: 22px;
}

/* PDF 舞台：canvas + 文字层 + 批注层 */
.pdf-scroll {
  flex: 1;
  overflow: auto;
  background: #e9ecee;
  display: flex;
  justify-content: center;
  padding: 24px 16px;
}

.pdf-stage {
  position: relative;
  display: inline-block;
}

.pdf-canvas {
  display: block;
  background: #fff;
  box-shadow: 0 2px 12px rgba(30, 43, 54, 0.16);
}

.tool-rect {
  cursor: crosshair;
}

/* 批注关闭：隐藏文字层，纯净阅读 */
.anno-off .textLayer {
  display: none;
}

/* 批注层：默认穿透（子元素可点），矩形模式整层拦截 */
.anno-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 3;
}

.tool-rect .anno-layer {
  pointer-events: auto;
}

.anno-rect {
  pointer-events: auto;
  cursor: pointer;
}

.anno-rect:hover {
  fill-opacity: 0.6;
  stroke: rgba(0, 0, 0, 0.35);
  stroke-width: 1;
}

/* 常显便签：标注旁的小卡片 */
.anno-pin {
  position: absolute;
  z-index: 4;
  display: flex;
  align-items: flex-start;
  gap: 5px;
  max-width: 190px;
  padding: 4px 8px;
  background: #fffdf4;
  border: 1px solid #e6e0c8;
  border-left-width: 3px;
  border-radius: 6px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.18);
  cursor: pointer;
  font-size: 12px;
  line-height: 1.5;
  color: #4b4636;
  transform: translateY(-100%);
  transition: box-shadow 0.15s, transform 0.15s;
}

.anno-pin:hover {
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.26);
  transform: translateY(-100%) scale(1.03);
}

.pin-dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-top: 4px;
}

.pin-text {
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  word-break: break-all;
}

/* 悬停信息卡 */
.anno-tip {
  position: absolute;
  z-index: 5;
  width: 200px;
  padding: 8px 10px;
  background: rgba(36, 41, 47, 0.92);
  color: #fff;
  border-radius: 8px;
  font-size: 12px;
  line-height: 1.6;
  pointer-events: none;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
}

.tip-head {
  font-weight: 600;
  opacity: 0.85;
  font-size: 11px;
  margin-bottom: 2px;
}

.tip-main {
  word-break: break-all;
}

.tip-quote {
  margin-top: 4px;
  opacity: 0.75;
  font-style: italic;
  word-break: break-all;
  border-top: 1px solid rgba(255, 255, 255, 0.15);
  padding-top: 4px;
}

.tip-hint {
  margin-top: 4px;
  opacity: 0.5;
  font-size: 11px;
}

.pdf-error-box {
  align-self: center;
  margin: 40px;
  max-width: 420px;
}

/* 批注列表 */
.anno-list-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 8px;
  border-bottom: 1px dashed #e6e9ef;
  cursor: pointer;
  border-radius: 6px;
  transition: background 0.15s;
}

.anno-list-item:hover {
  background: var(--el-color-primary-light-9);
}

.anno-dot {
  flex: none;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  margin-top: 4px;
  box-shadow: rgba(0, 0, 0, 0.15) 0 0 0 1px inset;
}

.anno-list-main {
  flex: 1;
  min-width: 0;
}

.anno-list-head {
  font-size: 12px;
  color: #8f959e;
  margin-bottom: 2px;
}

.anno-list-text {
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.edit-anno-meta {
  display: flex;
  align-items: center;
  gap: 8px;
}

.anno-quote {
  font-size: 12px;
  color: #8f959e;
  margin: 8px 0 0;
  line-height: 1.6;
}

/* ---------- 右侧 ---------- */
.right-pane {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--app-surface);
  position: relative;
}

.right-tabs {
  flex: none;
  padding: 0 12px;
  margin-bottom: 0;
}

.right-tabs :deep(.el-tabs__header) {
  margin-bottom: 0;
}

.right-tools {
  position: absolute;
  top: 8px;
  right: 16px;
  z-index: 5;
}

.right-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 20px 24px 60px;
}

.right-scroll :deep(.md-body) {
  font-size: 15px;
  line-height: 1.75;
}

.mb {
  margin-bottom: 12px;
}

.dim-line {
  color: #8f959e;
  font-size: 12px;
  margin-bottom: 8px;
  line-height: 1.7;
}

/* 证据核查 */
.evidence-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 7px 0;
  border-bottom: 1px dashed #e6e9ef;
}

.evidence-context {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: #8f959e;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.evidence-ops {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
}

.note-drop-zone {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.note-drop-zone :deep(.md-editor-wrap) {
  flex: 1;
}

/* 拖入高亮 */
.note-drop-zone:drag-over {
  outline: 2px dashed var(--el-color-primary);
}

/* 笔记与编辑 */
/* 编辑器撑满右侧可视高度：right-scroll(flex:1) → wrap(flex:1) 逐级传递 */
.notes-wrap,
.edit-wrap {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.mono :deep(textarea) {
  font-family: Consolas, 'Microsoft YaHei', monospace;
  font-size: 13px;
  line-height: 1.6;
}

@media (max-width: 1100px) {
  .pdf-toolbar {
    flex-wrap: wrap;
    overflow-x: visible;
  }

  .pdf-toolbar .toolbar-spacer {
    display: none;
  }
}

.edit-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>

<style>
/* pdf.js 文字层（全局：pdf.js 直接操作 DOM），透明文字支持选中 */
.textLayer {
  position: absolute;
  inset: 0;
  overflow: hidden;
  line-height: 1;
  text-align: initial;
  forced-color-adjust: none;
  transform-origin: 0 0;
  z-index: 2;
}

.textLayer span,
.textLayer br {
  color: transparent;
  position: absolute;
  white-space: pre;
  cursor: text;
  transform-origin: 0 0;
}

.textLayer ::selection {
  background: rgba(64, 158, 255, 0.35);
}
</style>
