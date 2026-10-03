# -*- coding: utf-8 -*-
"""批次4 修复：viewport 级旋转 + 双页按组计算可见范围 + 文字层对齐"""
import io

p = "src/components/pdf/PdfReader.vue"
s = io.open(p, encoding="utf-8").read()

# ---------- 1) renderPage：viewport 级旋转（去掉 CSS 后处理） ----------
OLD_RENDER = """async function renderPage(index: number) {
  const doc = props.pdfDoc
  const st = pages.value[index - 1]
  if (!doc || !st || st.rendered || st.rendering) return
  st.rendering = true
  try {
    const page = await doc.getPage(index)
    const dpr = Math.min(window.devicePixelRatio || 1, 2.5)
    const vp = page.getViewport({ scale: zoom.value })
    const rvp = page.getViewport({ scale: zoom.value * dpr })"""
NEW_RENDER = """async function renderPage(index: number) {
  const doc = props.pdfDoc
  const st = pages.value[index - 1]
  if (!doc || !st || st.rendered || st.rendering) return
  st.rendering = true
  try {
    const page = await doc.getPage(index)
    const dpr = Math.min(window.devicePixelRatio || 1, 2.5)
    // viewport 级旋转：pdf.js 原生处理（canvas 正向渲染，宽高自动换算，文字层同步对齐）
    const rot = rotation.value
    const vp = page.getViewport({ scale: zoom.value, rotation: rot })
    const rvp = page.getViewport({ scale: zoom.value * dpr, rotation: rot })"""
assert OLD_RENDER in s, "render anchor missing"
s = s.replace(OLD_RENDER, NEW_RENDER, 1)

# 去掉 canvas 挂载后的 CSS 旋转块
OLD_CSS_ROT = """    // 旋转显示（canvas 内容不变，CSS 变换 + 居中）
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
assert OLD_CSS_ROT in s, "css rot block missing"
s = s.replace(OLD_CSS_ROT, "", 1)

# ---------- 2) initPages：页尺寸带当前旋转（页面切换旋转后重算占位） ----------
OLD_INIT = """    const page = await doc.getPage(i)
    const vp = page.getViewport({ scale: 1 })"""
NEW_INIT = """    const page = await doc.getPage(i)
    const vp = page.getViewport({ scale: 1, rotation: rotation.value })"""
s = s.replace(OLD_INIT, NEW_INIT)

# ---------- 3) visibleRange / syncVisible：按组累加（双页正确判定可见性） ----------
OLD_VISIBLE = """function visibleRange(): { first: number; last: number } {
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
}"""
NEW_VISIBLE = """/** 组高度：组内最高页 + 间距（双页正确判定可见性）。 */
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
}"""
assert OLD_VISIBLE in s, "visibleRange missing"
s = s.replace(OLD_VISIBLE, NEW_VISIBLE, 1)

# ---------- 4) tick：也按组跟踪页码 ----------
OLD_TICK_CENTER = """        const center = top + (scrollDir.value === 'horizontal' ? root.clientWidth : root.clientHeight) / 2
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
        }"""
NEW_TICK_CENTER = """        const center = top + (scrollDir.value === 'horizontal' ? root.clientWidth : root.clientHeight) / 2
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
        }"""
assert OLD_TICK_CENTER in s, "tick center missing"
s = s.replace(OLD_TICK_CENTER, NEW_TICK_CENTER, 1)

# ---------- 5) setZoom 锚点也按组 ----------
OLD_ZOOM_ANCHOR = """    const center = root.scrollTop + root.clientHeight / 2
    let acc = 0
    for (const p of pages.value) {
      const h = pageHeight(p)
      if (center <= acc + h) {
        anchor = { page: p.index, ratio: (center - acc) / h }
        break
      }
      acc += h + PAGE_GAP
    }"""
NEW_ZOOM_ANCHOR = """    const horizontal = scrollDir.value === 'horizontal'
    const center = (horizontal ? root.scrollLeft : root.scrollTop) + (horizontal ? root.clientWidth : root.clientHeight) / 2
    let acc = 0
    for (const g of pageGroups.value) {
      const h = groupHeight(g)
      if (center <= acc + h) {
        anchor = { page: g[0], ratio: (center - acc) / h }
        break
      }
      acc += h
    }"""
assert OLD_ZOOM_ANCHOR in s, "zoom anchor missing"
s = s.replace(OLD_ZOOM_ANCHOR, NEW_ZOOM_ANCHOR, 1)

# ---------- 6) rotatePage：重算页尺寸 + 重渲染（viewport 旋转导致 cssW/H 互换） ----------
OLD_ROTATE = """function rotatePage() {
  rotation.value = (rotation.value + 90) % 360
  // 旋转后占位宽高互换 → 触发重排（高度由计算函数响应）
  for (const idx of Array.from(renderedCanvases.keys())) {
    const st = pages.value[idx - 1]
    if (st) st.rendered = false
    destroyPage(idx)
  }
  syncVisible()
}"""
NEW_ROTATE = """async function rotatePage() {
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
}"""
assert OLD_ROTATE in s, "rotate missing"
s = s.replace(OLD_ROTATE, NEW_ROTATE, 1)

# ---------- 7) dispW/dispH 不再需要旋转判断（viewport 已处理） ----------
OLD_DISP = """function dispW(p: { cssW: number; cssH: number }): number {
  const w = pageWidth(p as { cssW: number })
  const h = pageHeight(p as { cssH: number })
  return rotation.value === 90 || rotation.value === 270 ? h : w
}
function dispH(p: { cssW: number; cssH: number }): number {
  const w = pageWidth(p as { cssW: number })
  const h = pageHeight(p as { cssH: number })
  return rotation.value === 90 || rotation.value === 270 ? w : h
}"""
NEW_DISP = """// viewport 级旋转后 cssW/cssH 已是旋转后的尺寸，直接使用
function dispW(p: { cssW: number; cssH: number }): number {
  return pageWidth(p as { cssW: number })
}
function dispH(p: { cssW: number; cssH: number }): number {
  return pageHeight(p as { cssH: number })
}"""
assert OLD_DISP in s, "disp missing"
s = s.replace(OLD_DISP, NEW_DISP, 1)

# ---------- 8) page-group 样式：加 align-items:flex-start 防拉伸 + 双页垂直模式横向排 ----------
OLD_GROUP_CSS = """/* 双页分组 */
.page-group {
  display: flex;
  gap: 14px;
  flex: none;
  justify-content: center;
}"""
NEW_GROUP_CSS = """/* 双页分组：垂直模式=横向排列（左右页），水平模式=纵向（因外层已转横向 flex） */
.page-group {
  display: flex;
  gap: 14px;
  flex: none;
  justify-content: center;
  align-items: flex-start;
}"""
s = s.replace(OLD_GROUP_CSS, NEW_GROUP_CSS)

# rotated class 不再需要特殊样式（viewport 已处理），但保留 class 供调试
io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("fix ok")
