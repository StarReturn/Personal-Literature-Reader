# -*- coding: utf-8 -*-
"""批次4 script 补丁：双页/水平滚动/旋转/深色模式/拖拽入笔记"""
import io

p = "src/components/pdf/PdfReader.vue"
s = io.open(p, encoding="utf-8").read()

# ---------- 1) 状态扩展：页面布局 / 滚动方向 / 旋转 / 深色 ----------
OLD = """const containerRef = ref<HTMLElement | null>(null)
const zoom = ref(props.initialZoom || 1)
const mode = ref<'vertical' | 'paged'>(props.initialMode === 'paged' ? 'paged' : 'vertical')
const currentPage = ref(props.initialPage || 1)
const activeColor = ref<string>('yellow')"""
NEW = """const containerRef = ref<HTMLElement | null>(null)
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

function rotatePage() {
  rotation.value = (rotation.value + 90) % 360
  // 旋转后占位宽高互换 → 触发重排（高度由计算函数响应）
  for (const idx of Array.from(renderedCanvases.keys())) {
    const st = pages.value[idx - 1]
    if (st) st.rendered = false
    destroyPage(idx)
  }
  syncVisible()
}

// 旋转后的页宽高（占位尺寸）
function dispW(p: { cssW: number; cssH: number }): number {
  const w = pageWidth(p as { cssW: number })
  const h = pageHeight(p as { cssH: number })
  return rotation.value === 90 || rotation.value === 270 ? h : w
}
function dispH(p: { cssW: number; cssH: number }): number {
  const w = pageWidth(p as { cssW: number })
  const h = pageHeight(p as { cssH: number })
  return rotation.value === 90 || rotation.value === 270 ? w : h
}

// ---------- 拖选文字入笔记 ----------
const dragPayload = ref<{ quote: string; page: number } | null>(null)

function onDragStart(e: DragEvent) {
  const sel = window.getSelection()
  if (!sel || sel.isCollapsed) return
  const host = sel.anchorNode?.parentElement?.closest('.text-host')
  if (!host) return
  const pageEl = host.closest('.pdf-page-item') as HTMLElement
  dragPayload.value = { quote: sel.toString().replace(/\\s+/g, ' ').trim(), page: Number(pageEl?.dataset.page || 0) }
  e.dataTransfer?.setData('text/plain', dragPayload.value.quote)
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copyLink'
}"""
assert OLD in s, "state anchor missing"
s = s.replace(OLD, NEW, 1)

# ---------- 2) 滚动容器：分组渲染 + 旋转样式 + 拖拽事件 ----------
# 页元素循环替换为分组循环
OLD_PAGES = """        <div
          v-for="p in pages"
          :key="p.index"
          class="pdf-page-item"
          :data-page="p.index"
          :class="{ flash: flashPage === p.index }"
          :style="{ height: pageHeight(p) + 'px', width: pageWidth(p) + 'px' }"
        >"""
NEW_PAGES = """        <div
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
        >"""
assert OLD_PAGES in s, "pages loop missing"
s = s.replace(OLD_PAGES, NEW_PAGES, 1)

# 页元素关闭：搜索高亮和 badge 之后补 group 关闭 div
OLD_BADGE = """          <span class="page-badge">{{ p.index }}</span>
        </div>"""
NEW_BADGE = """          <span class="page-badge">{{ p.index }}</span>
        </div>
        </div>"""
# 注意：批次3 在 badge 前加了搜索高亮——关闭标签在 badge 后
assert OLD_BADGE in s, "badge close missing"
s = s.replace(OLD_BADGE, NEW_BADGE, 1)

# ---------- 3) 滚动容器 class/事件 ----------
OLD_SCROLL = """      <div
        ref="containerRef"
        class="reader-scroll"
        :class="{ paged: mode === 'paged', 'tool-ink': tool === 'ink' }"
        @wheel="onWheel"
        @mouseup="onMouseUp"
        @pointerdown="onInkPointerDown"
        @pointermove="onInkPointerMove"
        @pointerup="onInkPointerUp"
      >"""
NEW_SCROLL = """      <div
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
      >"""
assert OLD_SCROLL in s, "scroll anchor missing"
s = s.replace(OLD_SCROLL, NEW_SCROLL, 1)

# ---------- 4) canvas 旋转：canvas-slot 样式带旋转变换（渲染仍正向，CSS 旋转显示） ----------
# 修改 renderPage 中挂载后设置 transform
OLD_MOUNT = """    const slot = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .canvas-slot`)
    if (slot && slot.firstChild !== canvas) {
      slot.innerHTML = ''
      slot.appendChild(canvas)
    }"""
NEW_MOUNT = """    const slot = containerRef.value?.querySelector(`.pdf-page-item[data-page="${index}"] .canvas-slot`)
    if (slot && slot.firstChild !== canvas) {
      slot.innerHTML = ''
      slot.appendChild(canvas)
    }
    // 旋转显示（canvas 内容不变，CSS 变换 + 居中）
    if (slot) {
      const item = slot.closest('.pdf-page-item') as HTMLElement | null
      if (item) {
        const rot = rotation.value
        canvas.style.transform = rot ? `rotate(${rot}deg)` : ''
        if (rot === 90 || rot === 270) {
          canvas.style.position = 'absolute'
          canvas.style.left = '50%'
          canvas.style.top = '50%'
          canvas.style.transform += ` translate(-50%, -50%)`
          canvas.style.transform += rot === 90 ? ` translate(${(canvas.offsetHeight - canvas.offsetWidth) / 2}px, ${(canvas.offsetWidth - canvas.offsetHeight) / 2}px)` : ` translate(${(canvas.offsetWidth - canvas.offsetHeight) / 2}px, ${(canvas.offsetHeight - canvas.offsetWidth) / 2}px)`
        } else {
          canvas.style.position = ''
          canvas.style.left = ''
          canvas.style.top = ''
        }
      }
    }"""
assert OLD_MOUNT in s, "mount anchor missing"
s = s.replace(OLD_MOUNT, NEW_MOUNT, 1)

# ---------- 5) onMounted：系统深色监听 ----------
OLD_MOUNTED = """  intervalId = setInterval(tick, 150)
  window.addEventListener('keydown', onKeydown)
})"""
NEW_MOUNTED = """  intervalId = setInterval(tick, 150)
  window.addEventListener('keydown', onKeydown)
  watchSystemDark()
})"""
assert OLD_MOUNTED in s
s = s.replace(OLD_MOUNTED, NEW_MOUNTED, 1)

# ---------- 6) tick/visibleRange 支持横向（scrollLeft）—— 简化：横向时用 scrollLeft 当 scrollTop ----------
OLD_TICK = """      const top = root.scrollTop
      if (Math.abs(top - lastSyncedTop) > 24) {
        lastSyncedTop = top
        syncVisible()
        const center = top + root.clientHeight / 2"""
NEW_TICK = """      const top = scrollDir.value === 'horizontal' ? root.scrollLeft : root.scrollTop
      if (Math.abs(top - lastSyncedTop) > 24) {
        lastSyncedTop = top
        syncVisible()
        const center = top + (scrollDir.value === 'horizontal' ? root.clientWidth : root.clientHeight) / 2"""
assert OLD_TICK in s
s = s.replace(OLD_TICK, NEW_TICK, 1)

io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("script ok")
