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

// 批次4：页面布局 / 滚动方向 / 旋转 / 深色
type PageLayout = 'single' | 'double' | 'double-cover'
const pageLayout = ref<PageLayout>('single')
const scrollDir = ref<'vertical' | 'horizontal'>('vertical')
const rotation = ref(0) // 0 | 90 | 180 | 270
type DarkMode = 'light' | 'dark' | 'system'
const darkSetting = ref<DarkMode>('light')
const systemDark = ref(false)
const isDark = computed(() =>
  darkSetting.value === 'dark' || (darkSetting.value === 'system' && systemDark.value)
)

let darkMq: MediaQueryList | null = null
function watchSystemDark() {
  darkMq = window.matchMedia('(prefers-color-scheme: dark)')
  systemDark.value = darkMq.matches
  darkMq.addEventListener?.('change', (e) => (systemDark.value = e.matches))
}

// 双页分组：double=1,2 3,4…；double-cover=1 | 2,3 4,5…
const pageGroups = computed(() => {
  const n = pages.value.length
  const groups: number[][] = []
  if (pageLayout.value === 'single') {
    for (let i = 1; i <= n; i++) groups.push([i])
  } else if (pageLayout.value === 'double') {
    for (let i = 1; i <= n; i += 2) groups.push(i + 1 <= n ? [i, i + 1] : [i])
  } else {
    groups.push([1])
    for (let i = 2; i <= n; i += 2) groups.push(i + 1 <= n ? [i, i + 1] : [i])
  }
  return groups
})

function groupOf(page: number): number {
  return pageGroups.value.findIndex((g) => g.includes(page))
}

async function rotatePage() {
  rotation.value = (rotation.value + 90) % 360
  // viewport 级旋转 → 重算每页 cssW/cssH（宽高互换）→ 全量重渲染
  const doc = props.pdfDoc
  if (!doc) return
  for (const idx of Array.from(renderedCanvases.keys())) destroyPage(idx)
  const st = pages.value[0]
  if (st) {
    const page = await doc.getPage(1)
    const vp = page.getViewport({ scale: 1, rotation: rotation.value })
    for (const p of pages.value) {
      p.cssW = vp.width
      p.cssH = vp.height
      p.rendered = false
    }
  }
  await nextTick()
  syncVisible()
}

// 旋转后的页宽高（占位尺寸）
// viewport 级旋转后 cssW/cssH 已是旋转后的尺寸，直接使用
function dispW(p: { cssW: number; cssH: number }): number {
  return pageWidth(p as { cssW: number })
}
function dispH(p: { cssW: number; cssH: number }): number {
  return pageHeight(p as { cssH: number })
}

// ---------- 拖选文字入笔记 ----------
const dragPayload = ref<{ quote: string; page: number } | null>(null)

function onDragStart(e: DragEvent) {
  const sel = window.getSelection()
  if (!sel || sel.isCollapsed) return
  const host = sel.anchorNode?.parentElement?.closest('.text-host')
  if (!host) return
  const pageEl = host.closest('.pdf-page-item') as HTMLElement
  dragPayload.value = { quote: sel.toString().replace(/\s+/g, ' ').trim(), page: Number(pageEl?.dataset.page || 0) }
  e.dataTransfer?.setData('text/plain', dragPayload.value.quote)
  e.dataTransfer?.setData('app/page', String(dragPayload.value.page))
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copyLink'
}

// 工具状态：select=选择文字 / ink=墨迹
const tool = ref<'select' | 'ink'>('select')
const inkDrawing = ref<{ page: number; path: AnnotationPoint[] } | null>(null)

// 侧栏三态：annotations | thumbnails | outline
type SidebarMode = 'annotations' | 'thumbnails' | 'outline'
const sidebarOpen = ref(false)
const sidebarMode = ref<SidebarMode>('annotations')
const filterKind = ref('')
const filterColor = ref('')
const flashPage = ref<number | null>(null)

function switchSidebar(m: SidebarMode) {
  if (sidebarMode.value === m && sidebarOpen.value) {
    sidebarOpen.value = false
  } else {
    sidebarMode.value = m
    sidebarOpen.value = true
  }
}

// ---------- 缩略图 ----------
const thumbPages = ref<{ index: number; w: number; h: number }[]>([])
const thumbQueue: number[] = []
let thumbWorking = false
const renderedThumbs = new Set<number>()

async function buildThumbs() {
  const doc = props.pdfDoc
  if (!doc || thumbPages.value.length) return
  const list: { index: number; w: number; h: number }[] = []
  for (let i = 1; i <= doc.numPages; i++) {
    const page = await doc.getPage(i)
    const vp = page.getViewport({ scale: 1, rotation: rotation.value })
    list.push({ index: i, w: vp.width, h: vp.height })
  }
  thumbPages.value = list
  queueThumbs(currentPage.value)
}

function queueThumbs(near: number) {
  const order = [...thumbPages.value.map((t) => t.index)].sort(
    (a, b) => Math.abs(a - near) - Math.abs(b - near)
  )
  for (const i of order) {
    if (!thumbQueue.includes(i)) thumbQueue.push(i)
  }
  pumpThumbs()
}

async function pumpThumbs() {
  if (thumbWorking) return
  thumbWorking = true
  while (thumbQueue.length) {
    const idx = thumbQueue.shift()!
    if (renderedThumbs.has(idx)) continue
    await new Promise((r) => setTimeout(r, 30))
    await renderThumb(idx)
  }
  thumbWorking = false
}

async function renderThumb(index: number) {
  const doc = props.pdfDoc
  if (!doc) return
  const host = document.querySelector(`.thumb-item[data-page="${index}"] .thumb-slot`)
  if (!host) return
  try {
    const page = await doc.getPage(index)
    const scale = 150 / thumbPages.value[index - 1].w
    const vp = page.getViewport({ scale })
    const canvas = document.createElement('canvas')
    canvas.width = Math.floor(vp.width)
    canvas.height = Math.floor(vp.height)
    canvas.style.width = '100%'
    canvas.style.display = 'block'
    host.innerHTML = ''
    host.appendChild(canvas)
    await new Promise<void>((resolve) => {
      let settled = false
      const done = () => { if (!settled) { settled = true; resolve() } }
      page.render({ canvasContext: canvas.getContext('2d')!, viewport: vp }).promise.then(done, done)
      setTimeout(done, 1200)
    })
    renderedThumbs.add(index)
  } catch {
    /* 单页缩略图失败忽略 */
  }
}

function thumbAnnotations(page: number): string[] {
  const set = new Set(props.annotations.filter((a) => a.page === page).map((a) => colorHex(a.color)))
  return Array.from(set).slice(0, 4)
}

// ---------- 大纲 ----------
interface OutlineNode {
  title: string
  page: number
  children: OutlineNode[]
}
const outline = ref<OutlineNode[] | null>(null)

async function buildOutline() {
  const doc = props.pdfDoc
  if (!doc || outline.value !== null) return
  try {
    const raw = await doc.getOutline()
    if (!raw || !raw.length) {
      outline.value = []
      return
    }
    const toNode = async (item: { title: string; dest: unknown; items: unknown[] }): Promise<OutlineNode> => {
      let page = 1
      try {
        const dest = typeof item.dest === 'string' ? await doc.getDestination(item.dest) : item.dest
        if (Array.isArray(dest) && dest[0]) {
          const ref = dest[0]
          page = (typeof ref === 'object' && ref !== null ? await doc.getPageIndex(ref as { num: number; gen: number }) : 0) + 1
        }
      } catch {
        /* 目的地解析失败留在第 1 页 */
      }
      const children: OutlineNode[] = []
      if (Array.isArray(item.items)) {
        for (const child of item.items as { title: string; dest: unknown; items: unknown[] }[]) {
          children.push(await toNode(child))
        }
      }
      return { title: item.title, page, children }
    }
    const nodes: OutlineNode[] = []
    for (const item of raw as { title: string; dest: unknown; items: unknown[] }[]) {
      nodes.push(await toNode(item))
    }
    outline.value = nodes
  } catch {
    outline.value = []
  }
}

// ---------- Ctrl+F 全文搜索 ----------
interface SearchHit {
  page: number
  snippet: string
  rect: AnnotationRect
}
const searchOpen = ref(false)
const searchQuery = ref('')
const searchCase = ref(false)
const searchHits = ref<SearchHit[]>([])
const searchIndex: { page: number; text: string; items: { str: string; x: number; y: number; w: number; h: number }[] }[] = []
const searchActiveIdx = ref(-1)
const searchHighlight = ref<{ page: number; rect: AnnotationRect } | null>(null)
const indexedPages = new Set<number>()

async function ensurePageIndex(pageNum: number) {
  if (indexedPages.has(pageNum)) return
  indexedPages.add(pageNum)
  const doc = props.pdfDoc
  if (!doc) return
  try {
    const page = await doc.getPage(pageNum)
    const vp = page.getViewport({ scale: 1 })
    const tc = await page.getTextContent()
    const items = (tc.items as { str: string; transform: number[]; width: number; height: number }[]).map((it) => ({
      str: it.str,
      x: it.transform[4] / vp.width,
      y: (vp.height - it.transform[5] - it.height) / vp.height,
      w: it.width / vp.width,
      h: it.height / vp.height
    }))
    const text = items.map((i) => i.str).join(' ')
    searchIndex.push({ page: pageNum, text, items })
  } catch {
    /* 跳过 */
  }
}

async function buildSearchIndex() {
  const doc = props.pdfDoc
  if (!doc) return
  for (let i = 1; i <= doc.numPages; i++) await ensurePageIndex(i)
}

async function runSearch() {
  const q = searchQuery.value.trim()
  searchHits.value = []
  searchActiveIdx.value = -1
  searchHighlight.value = null
  if (q.length < 1) return
  if (searchIndex.length < (props.pdfDoc?.numPages || 0)) await buildSearchIndex()
  const needle = searchCase.value ? q : q.toLowerCase()
  for (const entry of searchIndex) {
    const hay = searchCase.value ? entry.text : entry.text.toLowerCase()
    let from = 0
    while (true) {
      const at = hay.indexOf(needle, from)
      if (at < 0) break
      let acc = 0
      let rect: AnnotationRect | null = null
      for (const it of entry.items) {
        if (at >= acc && at < acc + it.str.length + 1) {
          rect = { x: it.x - 0.005, y: it.y - 0.004, w: it.w + 0.01, h: it.h + 0.008 }
          break
        }
        acc += it.str.length + 1
      }
      const snippetStart = Math.max(0, at - 14)
      searchHits.value.push({
        page: entry.page,
        snippet: entry.text.slice(snippetStart, snippetStart + q.length + 28),
        rect: rect || { x: 0.4, y: 0.1, w: 0.2, h: 0.03 }
      })
      from = at + q.length
      if (searchHits.value.length > 300) break
    }
  }
}

function gotoHit(i: number) {
  const hit = searchHits.value[i]
  if (!hit) return
  searchActiveIdx.value = i
  gotoPage(hit.page)
  searchHighlight.value = { page: hit.page, rect: hit.rect }
}

function searchStep(dir: 1 | -1) {
  if (!searchHits.value.length) return
  const next = (searchActiveIdx.value + dir + searchHits.value.length) % searchHits.value.length
  gotoHit(next)
}

// ---------- 键盘快捷键 ----------
function focusSearchInput() {
  nextTick(() => {
    const el = document.querySelector('.search-panel input') as HTMLInputElement | null
    el?.focus()
    el?.select()
  })
}

function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    searchOpen.value = true
    focusSearchInput()
    return
  }
  if (searchOpen.value) {
    if (e.key === 'Escape') {
      searchOpen.value = false
      searchHighlight.value = null
    }
    return
  }
  const tag = (e.target as HTMLElement)?.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return
  if (e.key === 'PageDown' || e.key === 'j') { e.preventDefault(); pageStep(1) }
  else if (e.key === 'PageUp' || e.key === 'k') { e.preventDefault(); pageStep(-1) }
}

interface TextItemEx {
  str: string
  transform: number[]
  width: number
  height: number
}

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
    const vp = page.getViewport({ scale: 1, rotation: rotation.value })
    list.push({ index: i, cssW: vp.width, cssH: vp.height, rendered: false, rendering: false })
  }
  pages.value = list
  await nextTick()
  restorePosition()
}

// ---------- 懒渲染（滚动位置驱动；IntersectionObserver 在部分 WebView 不触发） ----------
/** 组高度：组内最高页 + 间距（双页正确判定可见性）。 */
function groupHeight(group: number[]): number {
  let max = 0
  for (const gi of group) {
    const p = pages.value[gi - 1]
    if (p) max = Math.max(max, dispH(p))
  }
  return max + PAGE_GAP
}

function visibleRange(): { first: number; last: number } {
  const root = containerRef.value
  if (!root || !pages.value.length) return { first: 1, last: 1 }
  const horizontal = scrollDir.value === 'horizontal'
  const top = horizontal ? root.scrollLeft : root.scrollTop
  const size = horizontal ? root.clientWidth : root.clientHeight
  const bottom = top + size
  let first = 1
  let last = 1
  let y = 0
  for (const g of pageGroups.value) {
    const h = groupHeight(g)
    if (y + h > top) {
      first = g[0]
      break
    }
    y += h
  }
  y = 0
  for (const g of pageGroups.value) {
    const h = groupHeight(g)
    if (y >= bottom) {
      break
    }
    y += h
    last = g[g.length - 1]
  }
  return { first: Math.max(1, first), last: Math.min(pages.value.length, last) }
}

let syncing = false
async function syncVisible() {
  if (syncing) return
  syncing = true
  try {
    const { first, last } = visibleRange()
    const BUFFER = 2
    const from = Math.max(1, first - BUFFER)
    const to = Math.min(pages.value.length, last + BUFFER)
    // 先销毁范围外页面
    for (const idx of Array.from(renderedCanvases.keys())) {
      if (idx < from || idx > to) destroyPage(idx)
    }
    // 串行渲染范围内页面（同一时刻只有一个 renderTask，避免 WebView 并发阻塞）
    for (let i = from; i <= to; i++) {
      const st = pages.value[i - 1]
      if (!st || st.rendered || st.rendering) continue
      await renderPage(i)
    }
  } finally {
    syncing = false
  }
}

// 直接渲染（由 syncVisible 的串行 for 循环逐页调用，保证同一时刻只有一个 renderTask）
/** 手动确定性文字层：本 WebView 的 pdf.js renderTextLayer/getTextContent
 *  异步 promise 间歇性挂起，故只消费一次性预取的 textContent 数据自行定位 span。
 *  位置由 item.transform 换算（scale1 坐标 × zoom），供选中生成注释。 */
function renderTextLayerWithContent(
  index: number,
  host: HTMLElement | null,
  tc: { items: TextItemEx[] },
  pageW1: number,
  pageH1: number
) {
  const target = host && host.isConnected ? host : containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .text-host`) as HTMLElement | null
  if (!target) return
  target.innerHTML = ''
  target.style.setProperty('--scale-factor', String(zoom.value))
  const frag = document.createDocumentFragment()
  for (const it of tc.items as TextItemEx[]) {
    if (!it.str) continue
    const [a, b, c, d, e, f] = it.transform
    const fontH = Math.hypot(b, d) || it.height || 10
    const span = document.createElement('span')
    span.textContent = it.str
    span.style.left = `${e * zoom.value}px`
    span.style.top = `${((pageH1 - f - fontH) / pageH1) * pageH1 * zoom.value}px`
    span.style.fontSize = `${fontH * zoom.value}px`
    span.style.fontFamily = 'sans-serif'
    span.style.width = `${it.width * zoom.value}px`
    frag.appendChild(span)
  }
  target.appendChild(frag)
  renderedTextLayers.add(index)
}

async function renderPage(index: number) {
  const doc = props.pdfDoc
  const st = pages.value[index - 1]
  if (!doc || !st || st.rendering) return
  // 已渲染但 canvas 因 Vue 重渲染脱离 DOM（切换布局会重建 v-for）→ 重新挂载即可
  if (st.rendered) {
    const orphan = renderedCanvases.get(index)
    if (orphan) {
      const slot = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .canvas-slot`)
      if (slot && slot.firstChild !== orphan) {
        slot.innerHTML = ''
        slot.appendChild(orphan)
      }
    }
    return
  }
  st.rendering = true
  try {
    const page = await doc.getPage(index)
    const dpr = Math.min(window.devicePixelRatio || 1, 2.5)
    // viewport 级旋转：pdf.js 原生处理（canvas 正向渲染，宽高自动换算，文字层同步对齐）
    const rot = rotation.value
    const vp = page.getViewport({ scale: zoom.value, rotation: rot })
    const rvp = page.getViewport({ scale: zoom.value * dpr, rotation: rot })
    let canvas = renderedCanvases.get(index)
    if (!canvas) {
      canvas = document.createElement('canvas')
      renderedCanvases.set(index, canvas)
    }
    canvas.width = Math.floor(rvp.width)
    canvas.height = Math.floor(rvp.height)
    canvas.style.width = `${Math.floor(vp.width)}px`
    canvas.style.height = `${Math.floor(vp.height)}px`
    // 注意：不在此处赋 st.cssW/cssH——initPages 已设置占位尺寸；
    // 此处赋值会触发 Vue 响应式重渲染 v-for，重建 DOM 导致手动 append 的 canvas 丢失
    // 先挂载再渲染：离屏 canvas 的 render 在部分 WebView 会挂起
    const slot = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .canvas-slot`)
    if (slot && slot.firstChild !== canvas) {
      slot.innerHTML = ''
      slot.appendChild(canvas)
    }

    if (slot) {
      // 关键顺序：先发起 getTextContent（与渲染任务并发会死锁，必须先取），
      // 再启动 canvas 渲染；最后用预取内容渲染文字层
      const tcPromise = page.getTextContent().catch(() => null)
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
      const tc = (await Promise.race([tcPromise, new Promise((r) => setTimeout(r, 3000))])) as { items: TextItemEx[] } | null
      if (tc) renderTextLayerWithContent(index, host2, tc, vp.width / zoom.value, vp.height / zoom.value)
    }
    st.rendered = true
  } catch {
    // 单页渲染失败不阻断其他页
  } finally {
    st.rendering = false
  }
}

/** 可视页文字层：支持选中文字生成注释。渲染期间 Vue 可能重建 DOM（切换布局），
 *  完成后检测 host 是否已脱离文档，脱离则用新 host 重试一次。 */
async function renderTextLayer(index: number, page: pdfjsLib.PDFPageProxy, viewport: pdfjsLib.PageViewport, retry = 0) {
  if (renderedTextLayers.has(index) && retry === 0) return
  const host = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .text-host`) as HTMLElement | null
  if (!host) return
  try {
    host.innerHTML = ''
    host.style.setProperty('--scale-factor', String(zoom.value))
    const tc = await page.getTextContent()
    // 超时兜底：renderTextLayer 的 promise 在本 WebView 同样可能不落定
    await Promise.race([
      pdfjsLib.renderTextLayer({
        textContentSource: tc,
        container: host,
        viewport,
        textDivs: []
      }).promise,
      new Promise((r) => setTimeout(r, 2500))
    ])
    // 孤儿检测：渲染期间 DOM 被重建 → 换新 host 重试
    if (!host.isConnected && retry < 2) {
      renderedTextLayers.delete(index)
      await renderTextLayer(index, page, viewport, retry + 1)
      return
    }
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
      const top = scrollDir.value === 'horizontal' ? root.scrollLeft : root.scrollTop
      if (Math.abs(top - lastSyncedTop) > 24) {
        lastSyncedTop = top
        syncVisible()
        const center = top + (scrollDir.value === 'horizontal' ? root.clientWidth : root.clientHeight) / 2
        let acc = 0
        let cur = 1
        for (const g of pageGroups.value) {
          const h = groupHeight(g)
          if (center <= acc + h) {
            // 组内取靠视口中心的那页（双页取左/先出现的）
            cur = g[0]
            break
          }
          acc += h
          cur = g[g.length - 1]
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
    const horizontal = scrollDir.value === 'horizontal'
    const center = (horizontal ? root.scrollLeft : root.scrollTop) + (horizontal ? root.clientWidth : root.clientHeight) / 2
    let acc = 0
    for (const g of pageGroups.value) {
      const h = groupHeight(g)
      if (center <= acc + h) {
        anchor = { page: g[0], ratio: (center - acc) / h }
        break
      }
      acc += h
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

watch(sidebarMode, (m) => {
  if (m === 'thumbnails') buildThumbs()
})

// 切换页面布局/方向/模式后组结构变化 → 全量拆除 canvas 重渲染（防孤儿元素）
watch([pageLayout, scrollDir, mode], () => {
  lastSyncedTop = -1
  renderedCanvases.forEach((c) => { c.width = 0 })
  renderedCanvases.clear()
  renderedTextLayers.clear()
  for (const p of pages.value) p.rendered = false
  nextTick(() => syncVisible())
})

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
    buildOutline()
  }
  intervalId = setInterval(tick, 150)
  window.addEventListener('keydown', onKeydown)
  watchSystemDark()
})

onBeforeUnmount(() => {
  if (intervalId) clearInterval(intervalId)
  window.removeEventListener('keydown', onKeydown)
})

const ZOOM_OPTIONS = [0.5, 0.75, 0.9, 1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4]
</script>

<template>
  <div class="pdf-reader">
    <!-- 工具栏 -->
    <div class="reader-toolbar">
      <el-button-group size="small">
        <el-button size="small" :type="sidebarOpen && sidebarMode === 'annotations' ? 'primary' : ''" :icon="Collection" title="注释列表" @click="switchSidebar('annotations')" />
        <el-button size="small" :type="sidebarOpen && sidebarMode === 'thumbnails' ? 'primary' : ''" title="页面缩略图" @click="switchSidebar('thumbnails')">▤</el-button>
        <el-button size="small" :type="sidebarOpen && sidebarMode === 'outline' ? 'primary' : ''" title="文档大纲" @click="switchSidebar('outline')">☰</el-button>
      </el-button-group>
      <el-button size="small" title="查找 (Ctrl+F)" @click="searchOpen = true; focusSearchInput()">⌕</el-button>
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
      <el-button-group size="small">
        <el-button size="small" :type="pageLayout === 'single' ? 'primary' : ''" title="单页视图" @click="pageLayout = 'single'">▯</el-button>
        <el-button size="small" :type="pageLayout === 'double' ? 'primary' : ''" title="双页视图" @click="pageLayout = 'double'">▯▯</el-button>
        <el-button size="small" :type="pageLayout === 'double-cover' ? 'primary' : ''" title="双页含封面（首页单独）" @click="pageLayout = 'double-cover'">◫</el-button>
      </el-button-group>
      <el-button-group size="small">
        <el-button size="small" :type="scrollDir === 'vertical' ? 'primary' : ''" :icon="Reading" title="垂直滚动" @click="scrollDir = 'vertical'" />
        <el-button size="small" :type="scrollDir === 'horizontal' ? 'primary' : ''" :icon="Sort" title="水平滚动" @click="scrollDir = 'horizontal'" />
        <el-button size="small" :type="mode === 'paged' ? 'primary' : ''" title="翻页模式（snap）" @click="mode = mode === 'vertical' ? 'paged' : 'vertical'; emit('mode-change', mode)">↷</el-button>
      </el-button-group>
      <el-button size="small" title="旋转 90°" @click="rotatePage">⟳</el-button>
      <el-button-group size="small">
        <el-button size="small" :type="darkSetting === 'light' ? 'primary' : ''" title="浅色" @click="darkSetting = 'light'">☀</el-button>
        <el-button size="small" :type="darkSetting === 'dark' ? 'primary' : ''" title="深色（PDF 反色护眼）" @click="darkSetting = 'dark'">🌙</el-button>
        <el-button size="small" :type="darkSetting === 'system' ? 'primary' : ''" title="跟随系统" @click="darkSetting = 'system'">◐</el-button>
      </el-button-group>
    </div>

    <div class="reader-body">
      <!-- 侧栏（三态：注释 / 缩略图 / 大纲） -->
      <aside v-if="sidebarOpen" class="anno-sidebar">
        <!-- 注释 -->
        <template v-if="sidebarMode === 'annotations'">
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
        </template>

        <!-- 缩略图 -->
        <template v-else-if="sidebarMode === 'thumbnails'">
          <div class="sidebar-head"><span>页面（{{ pageCount }}）</span></div>
          <div class="sidebar-list thumb-list">
            <div
              v-for="t in thumbPages"
              :key="t.index"
              class="thumb-item"
              :data-page="t.index"
              :class="{ active: t.index === currentPage }"
              @click="gotoPage(t.index)"
            >
              <div class="thumb-slot" :style="{ height: Math.round(150 * t.h / t.w) + 'px' }"></div>
              <span v-for="(c, ci) in thumbAnnotations(t.index)" :key="ci" class="thumb-dot" :style="{ background: c }"></span>
              <span class="thumb-num">{{ t.index }}</span>
            </div>
          </div>
        </template>

        <!-- 大纲 -->
        <template v-else>
          <div class="sidebar-head"><span>大纲</span></div>
          <div class="sidebar-list outline-list">
            <div v-if="outline === null" class="sidebar-empty">加载中…</div>
            <div v-else-if="outline.length === 0" class="sidebar-empty">此 PDF 没有内嵌大纲<br />可用左侧缩略图或页码导航</div>
            <template v-else>
              <div v-for="n0 in outline" :key="n0.title">
                <div class="outline-item" @click="gotoPage(n0.page)">
                  <span class="outline-title">{{ n0.title }}</span>
                  <span class="outline-page">{{ n0.page }}</span>
                </div>
                <div v-for="n1 in n0.children" :key="n1.title">
                  <div class="outline-item lv2" @click="gotoPage(n1.page)">
                    <span class="outline-title">{{ n1.title }}</span>
                    <span class="outline-page">{{ n1.page }}</span>
                  </div>
                  <div v-for="n2 in n1.children" :key="n2.title">
                    <div class="outline-item lv3" @click="gotoPage(n2.page)">
                      <span class="outline-title">{{ n2.title }}</span>
                      <span class="outline-page">{{ n2.page }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </template>
          </div>
        </template>
      </aside>

      <!-- 滚动容器 -->
      <div
        ref="containerRef"
        class="reader-scroll"
        :class="{
          paged: mode === 'paged',
          'tool-ink': tool === 'ink',
          horizontal: scrollDir === 'horizontal',
          dark: isDark
        }"
        @wheel="onWheel"
        @mouseup="onMouseUp"
        @pointerdown="onInkPointerDown"
        @pointermove="onInkPointerMove"
        @pointerup="onInkPointerUp"
        @dragstart="onDragStart"
      >
        <div
          v-for="g in pageGroups"
          :key="g[0]"
          class="page-group"
          :style="scrollDir === 'horizontal' ? { flexDirection: 'column' } : {}"
        >
        <div
          v-for="p in g.map((gi) => pages[gi - 1]).filter(Boolean)"
          :key="p!.index"
          class="pdf-page-item"
          :data-page="p!.index"
          :class="{ flash: flashPage === p!.index, rotated: rotation === 90 || rotation === 270 }"
          :style="{ height: dispH(p!) + 'px', width: dispW(p!) + 'px' }"
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
          <!-- 搜索命中高亮 -->
          <div
            v-if="searchHighlight && searchHighlight.page === p.index"
            class="search-hl"
            :style="{
              left: searchHighlight.rect.x * pageWidth(p) + 'px',
              top: searchHighlight.rect.y * pageHeight(p) + 'px',
              width: searchHighlight.rect.w * pageWidth(p) + 'px',
              height: searchHighlight.rect.h * pageHeight(p) + 'px'
            }"
          ></div>
          <span class="page-badge">{{ p.index }}</span>
        </div>
        </div>

        <!-- Ctrl+F 搜索面板 -->
        <div v-if="searchOpen" class="search-panel">
          <input
            v-model="searchQuery"
            class="search-input"
            placeholder="在文档中查找…  (Enter 下一处 / Shift+Enter 上一处 / Esc 关闭)"
            @keydown.enter.prevent="searchHits.length ? searchStep($event.shiftKey ? -1 : 1) : runSearch()"
            @keydown.esc="searchOpen = false; searchHighlight = null"
          />
          <button class="search-btn" title="大小写敏感" :class="{ on: searchCase }" @click="searchCase = !searchCase; runSearch()">Aa</button>
          <button class="search-btn" title="查找" @click="runSearch">⌕</button>
          <span class="search-count">
            {{ searchHits.length ? (searchActiveIdx >= 0 ? searchActiveIdx + 1 + ' / ' : '') + searchHits.length + ' 处' : (searchQuery ? '无结果' : '') }}
          </span>
          <button class="search-btn" :disabled="!searchHits.length" @click="searchStep(-1)">↑</button>
          <button class="search-btn" :disabled="!searchHits.length" @click="searchStep(1)">↓</button>
          <button class="search-btn" title="收起结果" v-if="searchHits.length">▾</button>
          <div v-if="searchHits.length" class="search-results">
            <div
              v-for="(h, i) in searchHits.slice(0, 50)"
              :key="i"
              class="search-result-item"
              :class="{ active: i === searchActiveIdx }"
              @click="gotoHit(i)"
            >
              <span class="sr-page">P{{ h.page }}</span>
              <span class="sr-text">{{ h.snippet }}</span>
            </div>
          </div>
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

/* 缩略图 */
.thumb-list {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 8px 6px;
}

.thumb-item {
  position: relative;
  width: 150px;
  background: #fff;
  border: 2px solid transparent;
  border-radius: 3px;
  cursor: pointer;
  box-shadow: 0 1px 5px rgba(0, 0, 0, 0.18);
}

.thumb-item:hover {
  border-color: var(--el-color-primary-light-5);
}

.thumb-item.active {
  border-color: var(--el-color-primary);
}

.thumb-slot {
  width: 100%;
  background: #f0f2f5;
  overflow: hidden;
}

.thumb-num {
  position: absolute;
  bottom: 3px;
  right: 5px;
  font-size: 10px;
  color: #666;
  background: rgba(255, 255, 255, 0.85);
  border-radius: 6px;
  padding: 0 5px;
}

.thumb-dot {
  position: absolute;
  top: 3px;
  left: 5px;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  margin-right: 2px;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.2);
}

.thumb-dot:nth-child(3) { left: 14px; }
.thumb-dot:nth-child(4) { left: 23px; }
.thumb-dot:nth-child(5) { left: 32px; }

/* 大纲 */
.outline-list {
  padding: 6px 4px;
}

.outline-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding: 5px 8px;
  border-radius: 5px;
  cursor: pointer;
  font-size: 12.5px;
}

.outline-item:hover {
  background: var(--el-color-primary-light-9);
}

.outline-item.lv2 { padding-left: 20px; }
.outline-item.lv3 { padding-left: 34px; }

.outline-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.outline-page {
  flex: none;
  color: #a8abb2;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

/* 搜索面板 */
.search-panel {
  position: absolute;
  top: 10px;
  right: 18px;
  z-index: 12;
  width: 340px;
  background: #fff;
  border: 1px solid #e2e5ea;
  border-radius: 8px;
  box-shadow: 0 6px 22px rgba(0, 0, 0, 0.18);
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  flex-wrap: wrap;
}

.search-input {
  flex: 1;
  min-width: 150px;
  border: 1px solid #e2e5ea;
  border-radius: 5px;
  padding: 4px 8px;
  font-size: 13px;
}

.search-input:focus {
  outline: none;
  border-color: var(--el-color-primary);
}

.search-btn {
  border: none;
  background: none;
  cursor: pointer;
  padding: 3px 6px;
  border-radius: 4px;
  color: #4b5058;
  font-size: 13px;
}

.search-btn:hover:not(:disabled) {
  background: #f0f2f5;
}

.search-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.search-btn.on {
  background: var(--el-color-primary-light-9);
  color: var(--el-color-primary);
  font-weight: 700;
}

.search-count {
  font-size: 11.5px;
  color: #8f959e;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.search-results {
  flex-basis: 100%;
  max-height: 220px;
  overflow-y: auto;
  border-top: 1px solid #eceef2;
  margin-top: 4px;
  padding-top: 4px;
}

.search-result-item {
  display: flex;
  gap: 7px;
  padding: 4px 6px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  align-items: baseline;
}

.search-result-item:hover {
  background: #f5f7fa;
}

.search-result-item.active {
  background: var(--el-color-primary-light-9);
}

.sr-page {
  flex: none;
  color: var(--el-color-primary);
  font-weight: 600;
  font-size: 11px;
}

.sr-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #4b5058;
}

/* 搜索命中高亮 */
.search-hl {
  position: absolute;
  z-index: 5;
  background: rgba(255, 213, 0, 0.45);
  border: 1px solid rgba(240, 170, 0, 0.85);
  border-radius: 2px;
  pointer-events: none;
}

/* 双页分组：垂直模式=横向排列（左右页），水平模式=纵向（因外层已转横向 flex） */
.page-group {
  display: flex;
  gap: 14px;
  flex: none;
  justify-content: center;
  align-items: flex-start;
}

.reader-scroll.horizontal {
  flex-direction: row;
  align-items: flex-start;
  overflow-x: auto;
  overflow-y: hidden;
}

.reader-scroll.horizontal .page-group {
  flex-direction: column;
}

/* 深色模式：PDF 反色（页面+文字层），注释色补偿 */
.reader-scroll.dark .canvas-slot,
.reader-scroll.dark .text-host {
  filter: invert(1) hue-rotate(180deg);
  background: #1b1b1b;
}

.reader-scroll.dark .anno-svg {
  filter: invert(1) hue-rotate(180deg) saturate(1.6);
}

.reader-scroll.dark .pdf-page-item {
  box-shadow: 0 3px 14px rgba(0, 0, 0, 0.7);
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
