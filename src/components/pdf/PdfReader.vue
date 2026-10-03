<script setup lang="ts">
// PdfReader：Zotero 式 PDF 阅读器（复刻批次 1 · 地基）。
// - 连续垂直滚动 + 翻页模式（scroll-snap 统一实现，一套渲染两种模式）
// - IntersectionObserver 懒渲染（可视 ±2 页），离屏销毁 canvas 控内存
// - DPR 物理像素高清；ctrl+滚轮缩放（锚点保持）；页宽适应/实际大小
// - 只读批注层（每页 SVG 色块，点击 emit 编辑）；批注创建在批次 2 升级
// - 位置记忆：中心页 + 缩放 + 模式（复用 papers.last_*）
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  ArrowLeft,
  FullScreen,
  Minus,
  Plus,
  Reading,
  Sort
} from '@element-plus/icons-vue'
import * as pdfjsLib from 'pdfjs-dist'
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.js?url'
import type { PdfAnnotation } from '../../ipc'

pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl

const props = defineProps<{
  pdfDoc: pdfjsLib.PDFDocumentProxy | null
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
}>()

const ANNO_HEX: Record<string, string> = {
  yellow: '#f5d76e',
  red: '#f28b82',
  blue: '#8ab4f8',
  green: '#81c995'
}

const containerRef = ref<HTMLElement | null>(null)
const zoom = ref(props.initialZoom || 1)
const mode = ref<'vertical' | 'paged'>(props.initialMode === 'paged' ? 'paged' : 'vertical')
const currentPage = ref(props.initialPage || 1)

interface PageState {
  index: number
  cssW: number
  cssH: number
  rendered: boolean
  rendering: boolean
}
const pages = ref<PageState[]>([])
const renderedCanvases = new Map<number, HTMLCanvasElement>()

const pageCount = computed(() => props.pdfDoc?.numPages || 0)

// ---------- 初始化页尺寸（占位高度，不渲染内容） ----------
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

// ---------- 懒渲染（滚动位置驱动；IntersectionObserver 在部分 WebView 不触发，故弃用） ----------
function visibleRange(): { first: number; last: number } {
  const root = containerRef.value
  if (!root || !pages.value.length) return { first: 1, last: 1 }
  const top = root.scrollTop
  const bottom = top + root.clientHeight
  let acc = 0
  let first = 1
  let last = 1
  const heights = pages.value.map((p) => pageHeight(p) + PAGE_GAP)
  const total = heights.reduce((a, b) => a + b, 0)
  // 首个可见页
  let y = 0
  for (let i = 0; i < heights.length; i++) {
    if (y + heights[i] > top) { first = i + 1; break }
    y += heights[i]
  }
  y = 0
  for (let i = 0; i < heights.length; i++) {
    if (y >= bottom) { last = i; break }
    y += heights[i]
    last = i + 1
  }
  last = Math.min(last, pages.value.length)
  void total
  return { first, last }
}

function syncVisible() {
  ;(window as any).__trace = (window as any).__trace || []
  ;(window as any).__trace.push('syncVisible')
  const { first, last } = visibleRange()
  ;(window as any).__trace.push('range ' + first + '-' + last)
  const BUFFER = 2
  const from = Math.max(1, first - BUFFER)
  const to = Math.min(pages.value.length, last + BUFFER)
  for (let i = from; i <= to; i++) renderPage(i)
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
      await page.render({ canvasContext: canvas.getContext('2d')!, viewport: rvp }).promise
    }
    st.rendered = true
  } catch {
    // 单页渲染失败不阻断其他页（缩放/资源异常）
  } finally {
    st.rendering = false
  }
}

function destroyPage(index: number) {
  const canvas = renderedCanvases.get(index)
  if (canvas) {
    canvas.width = 0
    canvas.height = 0
    renderedCanvases.delete(index)
  }
  const st = pages.value[index - 1]
  if (st) st.rendered = false
}

// ---------- 当前页跟踪 + 位置记忆（rAF 轮询：scroll 事件在部分 WebView 程序化滚动下不触发） ----------
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
    /* 单帧异常不中断轮询 */
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
  const root = containerRef.value
  if (!root || !pages.value.length) return
  gotoPage(props.initialPage || 1, 'auto')
  syncVisible()
}

// ---------- 对外：跳页 ----------
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
  // 重渲染可视页并保持锚点
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
    // 已渲染页按新 zoom 失效，由 syncVisible 按需重绘
    for (const idx of Array.from(renderedCanvases.keys())) {
      const st = pages.value[idx - 1]
      if (st) st.rendered = false
      destroyPage(idx)
    }
    syncVisible()
  })
}

// ctrl + 滚轮缩放
function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return
  e.preventDefault()
  setZoom(zoom.value * (e.deltaY < 0 ? 1.1 : 0.9))
}

// 适应页宽 / 实际大小
function fitWidth() {
  const root = containerRef.value
  const first = pages.value[0]
  if (!root || !first) return
  const avail = root.clientWidth - 48
  setZoom(avail / first.cssW)
}
function actualSize() {
  setZoom(1)
}

// 翻页按钮（paged 与 vertical 通用）
function pageStep(dir: 1 | -1) {
  gotoPage(Math.min(pageCount.value, Math.max(1, currentPage.value + dir)))
}

// 每页只读批注
function annotationsOf(page: number) {
  return props.annotations.filter((a) => a.page === page)
}

// ---------- 生命周期 ----------
watch(() => props.pdfDoc, async (doc) => {
  renderedCanvases.forEach((c) => { c.width = 0 })
  renderedCanvases.clear()
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
    <!-- Zotero 式工具栏 -->
    <div class="reader-toolbar">
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
        <el-button size="small" class="zoom-label" title="缩放比例">{{ Math.round(zoom * 100) }}%</el-button>
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
      <el-tooltip content="实际大小 (100%)" placement="bottom">
        <el-button size="small" @click="actualSize">1:1</el-button>
      </el-tooltip>
      <el-divider direction="vertical" />
      <el-tooltip :content="mode === 'vertical' ? '当前：连续滚动（点击切换为翻页模式）' : '当前：翻页模式（点击切换为连续滚动）'" placement="bottom">
        <el-button
          size="small"
          :type="mode === 'paged' ? 'primary' : ''"
          :icon="mode === 'paged' ? Sort : Reading"
          @click="mode = mode === 'vertical' ? 'paged' : 'vertical'; emit('mode-change', mode)"
        />
      </el-tooltip>
      <span class="dim toolbar-hint">Ctrl + 滚轮缩放</span>
    </div>

    <!-- 滚动容器 -->
    <div
      ref="containerRef"
      class="reader-scroll"
      :class="{ paged: mode === 'paged' }"
      @wheel="onWheel"
    >
      <div
        v-for="p in pages"
        :key="p.index"
        class="pdf-page-item"
        :data-page="p.index"
        :style="{ height: pageHeight(p) + 'px', width: pageWidth(p) + 'px' }"
      >
        <div class="canvas-slot"></div>
        <!-- 只读批注层 -->
        <svg class="anno-svg" :width="pageWidth(p)" :height="pageHeight(p)" :viewBox="`0 0 ${pageWidth(p)} ${pageHeight(p)}`">
          <g v-for="a in annotationsOf(p.index)" :key="a.id">
            <rect
              v-for="(r, ri) in a.rects"
              :key="ri"
              :x="r.x * pageWidth(p)"
              :y="r.y * pageHeight(p)"
              :width="r.w * pageWidth(p)"
              :height="r.h * pageHeight(p)"
              :fill="ANNO_HEX[a.color] || a.color"
              fill-opacity="0.42"
              class="anno-rect"
              @click.stop="emit('edit-annotation', a)"
            />
          </g>
        </svg>
        <span class="page-badge">{{ p.index }}</span>
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

.toolbar-hint {
  margin-left: auto;
}

.reader-scroll {
  flex: 1;
  overflow-y: auto;
  overflow-x: auto;
  padding: 16px 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
}

.reader-scroll.paged {
  scroll-snap-type: y mandatory;
}

.reader-scroll.paged .pdf-page-item {
  scroll-snap-align: start;
  scroll-snap-stop: always;
}

.pdf-page-item {
  position: relative;
  flex: none;
  background: #fff;
  box-shadow: 0 3px 14px rgba(0, 0, 0, 0.4);
  border-radius: 2px;
}

.canvas-slot {
  position: absolute;
  inset: 0;
}

.canvas-slot canvas {
  display: block;
  border-radius: 2px;
}

.anno-svg {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.anno-rect {
  pointer-events: auto;
  cursor: pointer;
}

.anno-rect:hover {
  fill-opacity: 0.6;
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
}
</style>
