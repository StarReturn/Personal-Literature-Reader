<script setup lang="ts">
// PdfReader：Zotero 式 PDF 阅读器。
// 批次1：连续滚动/翻页、懒渲染、DPR、锚点缩放、ctrl+滚轮、位置记忆
// 批次2：每页文字层 + 选中浮条（高亮/下划线/删除线/复制）、墨迹手绘、
//        常驻注释侧栏（筛选/跳页）、六类注释渲染（highlight/underline/strike/rect/note/ink）
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  ArrowLeft,
  Collection,
  EditPen,
  FullScreen,
  Minus,
  Plus,
  Reading,
  Sort
} from '@element-plus/icons-vue'
import * as pdfjsLib from 'pdfjs-dist'
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.js?url'
import type { PdfAnnotation, AnnotationRect, AnnotationPoint } from '../../ipc'
import { api } from '../../ipc'

pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl

const props = defineProps<{
  pdfDoc: pdfjsLib.PDFDocumentProxy | null
  paperId: string
  annotations: PdfAnnotation[]
  initialPage: number
  initialZoom: number
  initialMode: 'vertical' | 'paged'
}>()

const emit = defineEmits<{
  (e: 'page-change', page: number): void
  (e: 'zoom-change', zoom: number): void
  (e: 'mode-change', mode: 'vertical' | 'paged'): void
  (e: 'edit-annotation', a: PdfAnnotation): void
  (e: 'annotations-changed'): void
}>()

const PRESET = [
  { key: 'yellow', hex: '#f5d76e', label: '黄·重点' },
  { key: 'red', hex: '#f28b82', label: '红·质疑' },
  { key: 'blue', hex: '#8ab4f8', label: '蓝·同意' },
  { key: 'green', hex: '#81c995', label: '绿·待查' }
]
const ANNO_HEX: Record<string, string> = {
  yellow: '#f5d76e',
  red: '#f28b82',
  blue: '#8ab4f8',
  green: '#81c995'
}
const KIND_LABEL: Record<string, string> = {
  highlight: '高亮',
  underline: '下划线',
  strike: '删除线',
  rect: '框选',
  note: '便签',
  ink: '墨迹'
}
function colorHex(c: string): string {
  return ANNO_HEX[c] || c
}

const containerRef = ref<HTMLElement | null>(null)
const zoom = ref(props.initialZoom || 1)
const mode = ref<'vertical' | 'paged'>(props.initialMode === 'paged' ? 'paged' : 'vertical')
const currentPage = ref(props.initialPage || 1)
const activeColor = ref<string>('yellow')

// 工具状态：select=选择文字 / ink=墨迹
const tool = ref<'select' | 'ink'>('select')
const inkDrawing = ref<{ page: number; path: AnnotationPoint[] } | null>(null)

// 侧栏
const sidebarOpen = ref(false)
const filterKind = ref('')
const filterColor = ref('')
const flashPage = ref<number | null>(null)

interface PageState {
  index: number
  cssW: number
  cssH: number
  rendered: boolean
  rendering: boolean
}
const pages = ref<PageState[]>([])
const renderedCanvases = new Map<number, HTMLCanvasElement>()
const renderedTextLayers = new Set<number>()

const pageCount = computed(() => props.pdfDoc?.numPages || 0)

const sidebarList = computed(() =>
  props.annotations
    .filter((a) => (!filterKind.value || a.kind === filterKind.value) && (!filterColor.value || a.color === filterColor.value))
    .sort((x, y) => x.page - y.page)
)

// ---------- 初始化页尺寸 ----------
async function initPages() {
  const doc = props.pdfDoc
  if (!doc) return
  const list: PageState[] = []
  for (let i = 1; i <= doc.numPages; i++) {
    const page = await doc.getPage(i)
    const vp = page.getViewport({ scale: 1 })
    list.push({ index: i, cssW: vp.width, cssH: vp.height, rendered: false, rendering: false })
  }
  pages.value = list
  await nextTick()
  restorePosition()
}

// ---------- 懒渲染（滚动位置驱动；IntersectionObserver 在部分 WebView 不触发） ----------
function visibleRange(): { first: number; last: number } {
  const root = containerRef.value
  if (!root || !pages.value.length) return { first: 1, last: 1 }
  const top = root.scrollTop
  const bottom = top + root.clientHeight
  let first = 1
  let last = 1
  let y = 0
  for (let i = 0; i < pages.value.length; i++) {
    const h = pageHeight(pages.value[i]) + PAGE_GAP
    if (y + h > top) {
      first = i + 1
      break
    }
    y += h
  }
  y = 0
  for (let i = 0; i < pages.value.length; i++) {
    const h = pageHeight(pages.value[i]) + PAGE_GAP
    if (y >= bottom) {
      last = i
      break
    }
    y += h
    last = i + 1
  }
  return { first: Math.max(1, first), last: Math.min(pages.value.length, last) }
}

function syncVisible() {
  const { first, last } = visibleRange()
  const BUFFER = 2
  const from = Math.max(1, first - BUFFER)
  const to = Math.min(pages.value.length, last + BUFFER)
  for (let i = from; i <= to; i++) {
    renderPage(i)
  }
  for (const idx of Array.from(renderedCanvases.keys())) {
    if (idx < from || idx > to) destroyPage(idx)
  }
}

async function renderPage(index: number) {
  const doc = props.pdfDoc
  const st = pages.value[index - 1]
  if (!doc || !st || st.rendered || st.rendering) return
  st.rendering = true
  try {
    const page = await doc.getPage(index)
    const dpr = Math.min(window.devicePixelRatio || 1, 2.5)
    const vp = page.getViewport({ scale: zoom.value })
    const rvp = page.getViewport({ scale: zoom.value * dpr })
    let canvas = renderedCanvases.get(index)
    if (!canvas) {
      canvas = document.createElement('canvas')
      renderedCanvases.set(index, canvas)
    }
    canvas.width = Math.floor(rvp.width)
    canvas.height = Math.floor(rvp.height)
    canvas.style.width = `${Math.floor(vp.width)}px`
    canvas.style.height = `${Math.floor(vp.height)}px`
    st.cssW = Math.floor(vp.width)
    st.cssH = Math.floor(vp.height)
    // 先挂载再渲染：离屏 canvas 的 render 在部分 WebView 会挂起
    const slot = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .canvas-slot`)
    if (slot && slot.firstChild !== canvas) {
      slot.innerHTML = ''
      slot.appendChild(canvas)
    }
    if (slot) {
      // 本 WebView 中 renderTask 的 promise 可能不落定（绘制实际完成但回调丢失），
      // 用超时兜底：超时后继续文字层渲染
      await new Promise<void>((resolve) => {
        let settled = false
        const done = () => {
          if (!settled) {
            settled = true
            resolve()
          }
        }
        page.render({ canvasContext: canvas.getContext('2d')!, viewport: rvp }).promise.then(done, done)
        setTimeout(done, 2500)
      })
      await renderTextLayer(index, page, vp)
    }
    st.rendered = true
  } catch {
    // 单页渲染失败不阻断其他页
  } finally {
    st.rendering = false
  }
}

/** 可视页文字层：支持选中文字生成注释 */
async function renderTextLayer(index: number, page: pdfjsLib.PDFPageProxy, viewport: { width: number; height: number }) {
  if (renderedTextLayers.has(index)) return
  const host = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .text-host`)
  if (!host) return
  try {
    host.innerHTML = ''
    ;(host as HTMLElement).style.setProperty('--scale-factor', String(zoom.value))
    const tc = await page.getTextContent()
    const task = pdfjsLib.renderTextLayer({
      textContentSource: tc,
      container: host as HTMLElement,
      viewport,
      textDivs: []
    })
    await task.promise
    renderedTextLayers.add(index)
  } catch {
    // 扫描版 PDF 无文字层
  }
}

function destroyPage(index: number) {
  const canvas = renderedCanvases.get(index)
  if (canvas) {
    canvas.width = 0
    canvas.height = 0
    renderedCanvases.delete(index)
  }
  const host = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .text-host`)
  if (host) host.innerHTML = ''
  renderedTextLayers.delete(index)
  const st = pages.value[index - 1]
  if (st) st.rendered = false
}

// ---------- 页码跟踪（轮询：scroll 事件在部分 WebView 程序化滚动下不触发） ----------
let intervalId: ReturnType<typeof setInterval> | null = null
let lastSyncedTop = -1
function tick() {
  try {
    const root = containerRef.value
    if (root && pages.value.length) {
      const top = root.scrollTop
      if (Math.abs(top - lastSyncedTop) > 24) {
        lastSyncedTop = top
        syncVisible()
        const center = top + root.clientHeight / 2
        let acc = 0
        let cur = 1
        for (const p of pages.value) {
          const h = pageHeight(p)
          if (center <= acc + h) {
            cur = p.index
            break
          }
          acc += h + PAGE_GAP
          cur = p.index
        }
        if (cur !== currentPage.value) {
          currentPage.value = cur
          emit('page-change', cur)
        }
      }
    }
  } catch {
    /* 单帧异常不中断 */
  }
}

const PAGE_GAP = 14
function pageHeight(p: PageState): number {
  return Math.round(p.cssH * zoom.value)
}
function pageWidth(p: PageState): number {
  return Math.round(p.cssW * zoom.value)
}

function restorePosition() {
  gotoPage(props.initialPage || 1, 'auto')
  syncVisible()
}

// ---------- 跳页（对外暴露） ----------
function gotoPage(n: number, behavior: ScrollBehavior = 'smooth') {
  const root = containerRef.value
  if (!root || n < 1 || n > pages.value.length) return
  const el = root.querySelector(`.pdf-page-item[data-page="${n}"]`) as HTMLElement | null
  el?.scrollIntoView({ behavior, block: 'start' })
  currentPage.value = n
  emit('page-change', n)
}

defineExpose({ gotoPage, setZoom })

function setZoom(z: number, keepAnchor = true) {
  const root = containerRef.value
  let anchor: { page: number; ratio: number } | null = null
  if (root && keepAnchor) {
    const center = root.scrollTop + root.clientHeight / 2
    let acc = 0
    for (const p of pages.value) {
      const h = pageHeight(p)
      if (center <= acc + h) {
        anchor = { page: p.index, ratio: (center - acc) / h }
        break
      }
      acc += h + PAGE_GAP
    }
  }
  zoom.value = Math.min(4, Math.max(0.25, z))
  emit('zoom-change', zoom.value)
  nextTick(async () => {
    if (anchor && root) {
      let acc = 0
      for (const p of pages.value) {
        const h = pageHeight(p)
        if (p.index === anchor!.page) {
          root.scrollTop = acc + h * anchor!.ratio - root.clientHeight / 2
          break
        }
        acc += h + PAGE_GAP
      }
    }
    for (const idx of Array.from(renderedCanvases.keys())) {
      const st = pages.value[idx - 1]
      if (st) st.rendered = false
      destroyPage(idx)
    }
    syncVisible()
  })
}

function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return
  e.preventDefault()
  setZoom(zoom.value * (e.deltaY < 0 ? 1.1 : 0.9))
}

function fitWidth() {
  const root = containerRef.value
  const first = pages.value[0]
  if (!root || !first) return
  setZoom((root.clientWidth - 48) / first.cssW)
}
function actualSize() {
  setZoom(1)
}
function pageStep(dir: 1 | -1) {
  gotoPage(Math.min(pageCount.value, Math.max(1, currentPage.value + dir)))
}

// ---------- 选中浮条：高亮 / 下划线 / 删除线 / 复制 ----------
const pop = ref<{ x: number; y: number; rects: AnnotationRect[]; quote: string; page: number } | null>(null)

async function onMouseUp() {
  if (tool.value !== 'select') return
  await nextTick()
  const sel = window.getSelection()
  if (!sel || sel.isCollapsed || !sel.rangeCount) return
  const anchor = sel.anchorNode
  const host = anchor?.parentElement?.closest('.text-host')
  if (!host) return
  const pageEl = host.closest('.pdf-page-item') as HTMLElement
  const pageIndex = Number(pageEl?.dataset.page || 0)
  const canvasSlot = pageEl?.querySelector('.canvas-slot') as HTMLElement
  if (!pageIndex || !canvasSlot) return
  const base = canvasSlot.getBoundingClientRect()
  const rects: AnnotationRect[] = []
  const range = sel.getRangeAt(0)
  for (const r of Array.from(range.getClientRects())) {
    if (r.width < 1 || r.height < 1) continue
    const x = (r.left - base.left) / base.width
    const y = (r.top - base.top) / base.height
    if (x < -0.02 || x > 1.02 || y < -0.02 || y > 1.02) continue
    rects.push({
      x: Math.max(0, x),
      y: Math.max(0, y),
      w: Math.min(r.width / base.width, 1 - Math.max(0, x)),
      h: Math.min(r.height / base.height, 1 - Math.max(0, y))
    })
  }
  const quote = sel.toString().replace(/\s+/g, ' ').trim()
  if (!rects.length || !quote) return
  const root = containerRef.value!
  const rootRect = root.getBoundingClientRect()
  const first = range.getClientRects()[0]
  pop.value = {
    x: first.left - rootRect.left + root.scrollLeft,
    y: first.top - rootRect.top + root.scrollTop - 46,
    rects,
    quote,
    page: pageIndex
  }
}

async function applySelection(kind: 'highlight' | 'underline' | 'strike') {
  const p = pop.value
  if (!p) return
  pop.value = null
  window.getSelection()?.removeAllRanges()
  try {
    await api.addAnnotation(props.paperId, {
      page: p.page,
      kind,
      rects: p.rects,
      color: activeColor.value,
      quote: p.quote
    })
    emit('annotations-changed')
  } catch (e) {
    console.error(e)
  }
}

async function copySelection() {
  const p = pop.value
  if (!p) return
  try {
    await navigator.clipboard.writeText(p.quote)
  } catch {
    /* 剪贴板权限 */
  }
  pop.value = null
  window.getSelection()?.removeAllRanges()
}

// ---------- 墨迹 ----------
function onInkPointerDown(e: PointerEvent) {
  if (tool.value !== 'ink' || e.button !== 0) return
  const pageEl = (e.target as HTMLElement).closest('.pdf-page-item') as HTMLElement
  if (!pageEl) return
  const rect = pageEl.getBoundingClientRect()
  inkDrawing.value = {
    page: Number(pageEl.dataset.page),
    path: [{ x: (e.clientX - rect.left) / rect.width, y: (e.clientY - rect.top) / rect.height }]
  }
  ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
  e.preventDefault()
}

function onInkPointerMove(e: PointerEvent) {
  if (!inkDrawing.value) return
  const pageEl = (e.target as HTMLElement).closest('.pdf-page-item') as HTMLElement
  if (!pageEl || Number(pageEl.dataset.page) !== inkDrawing.value.page) return
  const rect = pageEl.getBoundingClientRect()
  inkDrawing.value.path.push({
    x: Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width)),
    y: Math.min(1, Math.max(0, (e.clientY - rect.top) / rect.height))
  })
}

async function onInkPointerUp() {
  const d = inkDrawing.value
  inkDrawing.value = null
  if (!d || d.path.length < 3) return
  try {
    await api.addAnnotation(props.paperId, {
      page: d.page,
      kind: 'ink',
      rects: [],
      strokes: [d.path],
      color: activeColor.value
    })
    emit('annotations-changed')
  } catch (e) {
    console.error(e)
  }
}

// ---------- 侧栏 ----------
function jumpToAnnotation(a: PdfAnnotation) {
  gotoPage(a.page)
  flashPage.value = a.page
  setTimeout(() => (flashPage.value = null), 1600)
}

function annotationsOf(page: number) {
  return props.annotations.filter((a) => a.page === page)
}

function inkPoints(a: PdfAnnotation, p: PageState): string {
  const W = pageWidth(p)
  const H = pageHeight(p)
  return (a.strokes || [])
    .map((path) => path.map((pt) => `${(pt.x * W).toFixed(1)},${(pt.y * H).toFixed(1)}`).join(' '))
    .join(' ')
}

// ---------- 生命周期 ----------
watch(() => props.pdfDoc, async (doc) => {
  renderedCanvases.forEach((c) => { c.width = 0 })
  renderedCanvases.clear()
  renderedTextLayers.clear()
  if (doc) {
    await initPages()
    await nextTick()
    syncVisible()
  }
})

onMounted(() => {
  if (props.pdfDoc) {
    initPages().then(() => nextTick()).then(() => syncVisible())
  }
  intervalId = setInterval(tick, 150)
})

onBeforeUnmount(() => {
  if (intervalId) clearInterval(intervalId)
})

const ZOOM_OPTIONS = [0.5, 0.75, 0.9, 1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4]
</script>

<template>
  <div class="pdf-reader">
    <!-- 工具栏 -->
    <div class="reader-toolbar">
      <el-tooltip content="注释列表" placement="bottom">
        <el-button size="small" :type="sidebarOpen ? 'primary' : ''" :icon="Collection" @click="sidebarOpen = !sidebarOpen" />
      </el-tooltip>
      <el-divider direction="vertical" />
      <el-button-group size="small">
        <el-button :icon="ArrowLeft" :disabled="currentPage <= 1" @click="pageStep(-1)" title="上一页" />
        <el-button :icon="ArrowLeft" class="flip-h" :disabled="currentPage >= pageCount" @click="pageStep(1)" title="下一页" />
      </el-button-group>
      <span class="page-pos">
        <input
          class="page-jump"
          type="number"
          min="1"
          :max="pageCount"
          :value="currentPage"
          @change="gotoPage(Number(($event.target as HTMLInputElement).value))"
        />
        <span class="dim">/ {{ pageCount }}</span>
      </span>
      <el-divider direction="vertical" />
      <el-button-group size="small">
        <el-button :icon="Minus" :disabled="zoom <= 0.25" @click="setZoom(zoom - 0.1)" title="缩小" />
        <el-button size="small" class="zoom-label">{{ Math.round(zoom * 100) }}%</el-button>
        <el-button :icon="Plus" :disabled="zoom >= 4" @click="setZoom(zoom + 0.1)" title="放大" />
      </el-button-group>
      <el-select
        :model-value="ZOOM_OPTIONS.includes(zoom) ? zoom : undefined"
        size="small"
        class="zoom-select"
        placeholder="比例"
        @change="(v: number) => setZoom(v)"
      >
        <el-option v-for="z in ZOOM_OPTIONS" :key="z" :label="Math.round(z * 100) + '%'" :value="z" />
      </el-select>
      <el-tooltip content="适应页宽" placement="bottom">
        <el-button size="small" :icon="FullScreen" @click="fitWidth" />
      </el-tooltip>
      <el-tooltip content="实际大小" placement="bottom">
        <el-button size="small" @click="actualSize">1:1</el-button>
      </el-tooltip>
      <el-divider direction="vertical" />
      <span class="color-swatches">
        <span
          v-for="c in PRESET"
          :key="c.key"
          class="swatch"
          :class="{ active: activeColor === c.key }"
          :style="{ background: c.hex }"
          :title="c.label"
          @click="activeColor = c.key"
        ></span>
      </span>
      <el-button-group size="small">
        <el-button
          size="small"
          :type="tool === 'select' ? 'primary' : ''"
          title="选择文字（选中后浮条：高亮/下划线/删除线/复制）"
          @click="tool = 'select'"
        >选择</el-button>
        <el-button
          size="small"
          :type="tool === 'ink' ? 'primary' : ''"
          :icon="EditPen"
          title="墨迹手绘（在页面上拖动绘制）"
          @click="tool = 'ink'"
        >墨迹</el-button>
      </el-button-group>
      <el-tooltip :content="mode === 'vertical' ? '连续滚动（点击切翻页）' : '翻页模式（点击切连续滚动）'" placement="bottom">
        <el-button
          size="small"
          :type="mode === 'paged' ? 'primary' : ''"
          :icon="mode === 'paged' ? Sort : Reading"
          @click="mode = mode === 'vertical' ? 'paged' : 'vertical'; emit('mode-change', mode)"
        />
      </el-tooltip>
    </div>

    <div class="reader-body">
      <!-- 常驻注释侧栏 -->
      <aside v-if="sidebarOpen" class="anno-sidebar">
        <div class="sidebar-head">
          <span>注释（{{ sidebarList.length }}）</span>
          <span class="sidebar-filters">
            <select v-model="filterKind" class="mini-select">
              <option value="">全部类型</option>
              <option v-for="(l, k) in KIND_LABEL" :key="k" :value="k">{{ l }}</option>
            </select>
            <select v-model="filterColor" class="mini-select">
              <option value="">全部颜色</option>
              <option v-for="c in PRESET" :key="c.key" :value="c.key">{{ c.label }}</option>
            </select>
          </span>
        </div>
        <div class="sidebar-list">
          <div v-if="sidebarList.length === 0" class="sidebar-empty">暂无注释<br />选中 PDF 文字或用墨迹工具添加</div>
          <div
            v-for="a in sidebarList"
            :key="a.id"
            class="sidebar-item"
            @click="jumpToAnnotation(a)"
          >
            <span class="item-dot" :style="{ background: colorHex(a.color) }"></span>
            <div class="item-main">
              <div class="item-head">
                第 {{ a.page }} 页 · {{ KIND_LABEL[a.kind] || a.kind }}
                <span v-for="t in a.tags" :key="t" class="item-tag">{{ t }}</span>
              </div>
              <div class="item-text">{{ a.text || a.quote || '（无备注）' }}</div>
            </div>
          </div>
        </div>
      </aside>

      <!-- 滚动容器 -->
      <div
        ref="containerRef"
        class="reader-scroll"
        :class="{ paged: mode === 'paged', 'tool-ink': tool === 'ink' }"
        @wheel="onWheel"
        @mouseup="onMouseUp"
        @pointerdown="onInkPointerDown"
        @pointermove="onInkPointerMove"
        @pointerup="onInkPointerUp"
      >
        <div
          v-for="p in pages"
          :key="p.index"
          class="pdf-page-item"
          :data-page="p.index"
          :class="{ flash: flashPage === p.index }"
          :style="{ height: pageHeight(p) + 'px', width: pageWidth(p) + 'px' }"
        >
          <div class="canvas-slot"></div>
          <div class="text-host textLayer"></div>
          <svg class="anno-svg" :width="pageWidth(p)" :height="pageHeight(p)" :viewBox="`0 0 ${pageWidth(p)} ${pageHeight(p)}`">
            <g v-for="a in annotationsOf(p.index)" :key="a.id">
              <template v-if="a.kind === 'highlight' || a.kind === 'rect' || a.kind === 'note'">
                <rect
                  v-for="(r, ri) in a.rects"
                  :key="ri"
                  :x="r.x * pageWidth(p)"
                  :y="r.y * pageHeight(p)"
                  :width="r.w * pageWidth(p)"
                  :height="r.h * pageHeight(p)"
                  :fill="colorHex(a.color)"
                  fill-opacity="0.42"
                  class="anno-rect"
                  @click.stop="emit('edit-annotation', a)"
                />
              </template>
              <template v-else-if="a.kind === 'underline'">
                <line
                  v-for="(r, ri) in a.rects"
                  :key="ri"
                  :x1="r.x * pageWidth(p)"
                  :y1="(r.y + r.h) * pageHeight(p)"
                  :x2="(r.x + r.w) * pageWidth(p)"
                  :y2="(r.y + r.h) * pageHeight(p)"
                  :stroke="colorHex(a.color)"
                  :stroke-width="2.5"
                  stroke-linecap="round"
                  class="anno-rect"
                  @click.stop="emit('edit-annotation', a)"
                />
              </template>
              <template v-else-if="a.kind === 'strike'">
                <line
                  v-for="(r, ri) in a.rects"
                  :key="ri"
                  :x1="r.x * pageWidth(p)"
                  :y1="(r.y + r.h / 2) * pageHeight(p)"
                  :x2="(r.x + r.w) * pageWidth(p)"
                  :y2="(r.y + r.h / 2) * pageHeight(p)"
                  :stroke="colorHex(a.color)"
                  :stroke-width="2"
                  stroke-linecap="round"
                  class="anno-rect"
                  @click.stop="emit('edit-annotation', a)"
                />
              </template>
              <polyline
                v-else-if="a.kind === 'ink'"
                :points="inkPoints(a, p)"
                fill="none"
                :stroke="colorHex(a.color)"
                stroke-width="2.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                class="anno-rect"
                @click.stop="emit('edit-annotation', a)"
              />
            </g>
          </svg>
          <span class="page-badge">{{ p.index }}</span>
        </div>

        <!-- 选中浮条 -->
        <div v-if="pop" class="selection-pop" :style="{ left: pop.x + 'px', top: pop.y + 'px' }">
          <span
            v-for="c in PRESET"
            :key="c.key"
            class="pop-color"
            :style="{ background: c.hex }"
            :title="c.label + '高亮'"
            @mousedown.prevent="activeColor = c.key; applySelection('highlight')"
          ></span>
          <span class="pop-divider"></span>
          <button class="pop-btn" title="下划线（当前颜色）" @mousedown.prevent="applySelection('underline')">U</button>
          <button class="pop-btn strike-text" title="删除线（当前颜色）" @mousedown.prevent="applySelection('strike')">S</button>
          <span class="pop-divider"></span>
          <button class="pop-btn" title="复制文字" @mousedown.prevent="copySelection">⧉</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pdf-reader {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  background: #3f4348;
}

.reader-toolbar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: #fff;
  border-bottom: 1px solid #e6e9ef;
  flex-wrap: wrap;
}

.flip-h :deep(.el-icon) {
  transform: scaleX(-1);
}

.page-pos {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-variant-numeric: tabular-nums;
}

.page-jump {
  width: 48px;
  padding: 2px 6px;
  border: 1px solid #e2e5ea;
  border-radius: 4px;
  text-align: center;
  font-size: 13px;
}

.page-jump:focus {
  outline: none;
  border-color: var(--el-color-primary);
}

.dim {
  color: #8f959e;
  font-size: 12px;
}

.zoom-label {
  min-width: 52px;
  pointer-events: none;
  font-variant-numeric: tabular-nums;
}

.zoom-select {
  width: 82px;
}

.color-swatches {
  display: inline-flex;
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
}

.swatch.active {
  box-shadow: 0 0 0 2px var(--el-color-primary);
}

.reader-body {
  flex: 1;
  display: flex;
  min-height: 0;
}

/* 注释侧栏 */
.anno-sidebar {
  flex: none;
  width: 218px;
  background: #fbfcfd;
  border-right: 1px solid #e6e9ef;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.sidebar-head {
  flex: none;
  padding: 8px 10px 6px;
  font-size: 13px;
  font-weight: 600;
  border-bottom: 1px solid #e6e9ef;
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 4px;
}

.sidebar-filters {
  display: flex;
  gap: 4px;
}

.mini-select {
  font-size: 11px;
  border: 1px solid #e2e5ea;
  border-radius: 4px;
  padding: 1px 3px;
  background: #fff;
  max-width: 88px;
}

.sidebar-list {
  flex: 1;
  overflow-y: auto;
  padding: 4px;
}

.sidebar-empty {
  color: #a8abb2;
  font-size: 12px;
  text-align: center;
  padding: 28px 8px;
  line-height: 2;
}

.sidebar-item {
  display: flex;
  gap: 7px;
  padding: 7px 6px;
  border-radius: 6px;
  cursor: pointer;
  border-bottom: 1px dashed #eceef2;
}

.sidebar-item:hover {
  background: var(--el-color-primary-light-9);
}

.item-dot {
  flex: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  margin-top: 4px;
}

.item-main {
  flex: 1;
  min-width: 0;
}

.item-head {
  font-size: 11px;
  color: #8f959e;
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}

.item-tag {
  background: var(--el-color-primary-light-9);
  color: var(--el-color-primary);
  border-radius: 6px;
  padding: 0 5px;
  font-size: 10px;
}

.item-text {
  font-size: 12px;
  line-height: 1.5;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  word-break: break-all;
}

/* 滚动区 */
.reader-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: auto;
  padding: 16px 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  position: relative;
  min-width: 0;
}

.reader-scroll.paged {
  scroll-snap-type: y mandatory;
}

.reader-scroll.paged .pdf-page-item {
  scroll-snap-align: start;
  scroll-snap-stop: always;
}

.reader-scroll.tool-ink {
  cursor: crosshair;
}

.pdf-page-item {
  position: relative;
  flex: none;
  background: #fff;
  box-shadow: 0 3px 14px rgba(0, 0, 0, 0.4);
  border-radius: 2px;
}

.pdf-page-item.flash {
  outline: 2px solid var(--el-color-primary);
  outline-offset: 2px;
}

.canvas-slot {
  position: absolute;
  inset: 0;
}

.canvas-slot canvas {
  display: block;
  border-radius: 2px;
}

.text-host {
  position: absolute;
  inset: 0;
  overflow: hidden;
  line-height: 1;
  z-index: 2;
}

.tool-ink .text-host {
  pointer-events: none;
}

.anno-svg {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 3;
}

.anno-rect {
  pointer-events: auto;
  cursor: pointer;
}

.anno-rect:hover {
  filter: brightness(1.1) saturate(1.4);
}

.page-badge {
  position: absolute;
  bottom: 4px;
  right: 8px;
  font-size: 11px;
  color: rgba(255, 255, 255, 0.85);
  background: rgba(0, 0, 0, 0.35);
  border-radius: 8px;
  padding: 0 7px;
  pointer-events: none;
  z-index: 4;
}

/* 选中浮条 */
.selection-pop {
  position: absolute;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 6px;
  background: #2b2f36;
  border-radius: 8px;
  padding: 5px 9px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
  user-select: none;
}

.pop-color {
  width: 17px;
  height: 17px;
  border-radius: 50%;
  cursor: pointer;
  border: 2px solid rgba(255, 255, 255, 0.75);
}

.pop-color:hover {
  transform: scale(1.15);
}

.pop-divider {
  width: 1px;
  height: 16px;
  background: rgba(255, 255, 255, 0.22);
}

.pop-btn {
  border: none;
  background: none;
  color: #e6e9ef;
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
  padding: 1px 4px;
  border-radius: 4px;
}

.pop-btn:hover {
  background: rgba(255, 255, 255, 0.14);
}

.strike-text {
  text-decoration: line-through;
}
</style>

<style>
/* pdf.js 文字层（全局） */
.textLayer {
  text-align: initial;
  forced-color-adjust: none;
  transform-origin: 0 0;
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
