# -*- coding: utf-8 -*-
"""批次3 补丁：侧栏三态/缩略图/大纲/搜索/快捷键"""
import io

p = "src/components/pdf/PdfReader.vue"
s = io.open(p, encoding="utf-8").read()

# ---------- 1) script 头部替换侧栏状态 ----------
OLD_SIDEBAR = """// 侧栏
const sidebarOpen = ref(false)
const filterKind = ref('')
const filterColor = ref('')
const flashPage = ref<number | null>(null)"""

NEW_SIDEBAR = r"""// 侧栏三态：annotations | thumbnails | outline
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
    const vp = page.getViewport({ scale: 1 })
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
}"""

assert OLD_SIDEBAR in s, "sidebar anchor missing"
s = s.replace(OLD_SIDEBAR, NEW_SIDEBAR, 1)

# ---------- 2) 生命周期 ----------
s = s.replace("""onMounted(() => {
  if (props.pdfDoc) {
    initPages().then(() => nextTick()).then(() => syncVisible())
  }
  intervalId = setInterval(tick, 150)
})""", """onMounted(() => {
  if (props.pdfDoc) {
    initPages().then(() => nextTick()).then(() => syncVisible())
    buildOutline()
  }
  intervalId = setInterval(tick, 150)
  window.addEventListener('keydown', onKeydown)
})""")

s = s.replace("""onBeforeUnmount(() => {
  if (intervalId) clearInterval(intervalId)
})""", """onBeforeUnmount(() => {
  if (intervalId) clearInterval(intervalId)
  window.removeEventListener('keydown', onKeydown)
})""")

s = s.replace("""// ---------- 生命周期 ----------
watch(() => props.pdfDoc, async (doc) => {""", """watch(sidebarMode, (m) => {
  if (m === 'thumbnails') buildThumbs()
})

// ---------- 生命周期 ----------
watch(() => props.pdfDoc, async (doc) => {""")

io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("script part ok")
