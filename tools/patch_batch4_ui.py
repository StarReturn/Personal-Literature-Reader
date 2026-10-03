# -*- coding: utf-8 -*-
"""批次4 UI 补丁：工具栏布局/方向/旋转/深色控件 + 样式 + ReadView 拖放接收 + 导出注释"""
import io

p = "src/components/pdf/PdfReader.vue"
s = io.open(p, encoding="utf-8").read()

# ---------- 1) 工具栏：模式按钮组替换为（布局/方向/旋转/深色） ----------
OLD_MODE_BTN = """      <el-tooltip :content="mode === 'vertical' ? '连续滚动（点击切翻页）' : '翻页模式（点击切连续滚动）'" placement="bottom">
        <el-button
          size="small"
          :type="mode === 'paged' ? 'primary' : ''"
          :icon="mode === 'paged' ? Sort : Reading"
          @click="mode = mode === 'vertical' ? 'paged' : 'vertical'; emit('mode-change', mode)"
        />
      </el-tooltip>"""
NEW_MODE_BTN = """      <el-button-group size="small">
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
      </el-button-group>"""
assert OLD_MODE_BTN in s, "mode btn missing"
s = s.replace(OLD_MODE_BTN, NEW_MODE_BTN, 1)

# ---------- 2) 样式：分组/横向/深色 ----------
OLD_CSS = """/* 选中浮条 */"""
NEW_CSS = """/* 双页分组 */
.page-group {
  display: flex;
  gap: 14px;
  flex: none;
  justify-content: center;
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

/* 选中浮条 */"""
assert OLD_CSS in s
s = s.replace(OLD_CSS, NEW_CSS, 1)

io.open(p, "w", encoding="utf-8", newline="\n").write(s)

# ================= ReadView：笔记编辑器接收拖放 =================
p2 = "src/views/ReadView.vue"
s2 = io.open(p2, encoding="utf-8").read()

# 笔记区包一个 drop 容器（MarkdownEditor 外层处理 dragover/drop）
OLD_NOTES = """          <MarkdownEditor v-model="noteText" placeholder="用 Markdown 记录你的研究判断、疑问和结论…（左侧编辑，右侧实时预览）" />"""
NEW_NOTES = """          <div
            class="note-drop-zone"
            @dragover.prevent
            @drop.prevent="onNoteDrop"
          >
            <MarkdownEditor v-model="noteText" placeholder="用 Markdown 记录你的研究判断、疑问和结论…（左侧编辑，右侧实时预览；可从 PDF 拖选文字进来）" />
          </div>"""
assert OLD_NOTES in s2, "notes anchor missing"
s2 = s2.replace(OLD_NOTES, NEW_NOTES, 1)

# onNoteDrop：接收拖入文本生成引用块（题录+页码+原文）
OLD_FN = """function addToCompare() {"""
NEW_FN = """/** PDF 拖选文字入笔记：生成引用块（题录 + 页码 + 原文）。 */
function onNoteDrop(e: DragEvent) {
  const text = e.dataTransfer?.getData('text/plain')?.trim()
  if (!text) return
  const paperTitle = paper.value?.title || '未知文献'
  const cite = `> ${text}\\n\\n—— ${paperTitle}${lastQuotePage.value ? `（PDF 第 ${lastQuotePage.value} 页）` : ''}\\n\\n`
  noteText.value = (noteText.value ? noteText.value + '\\n' : '') + cite
  noteDirty.value = true
  toastOk('已作为引用块加入笔记')
}

function addToCompare() {"""
assert OLD_FN in s2
s2 = s2.replace(OLD_FN, NEW_FN, 1)

# lastQuotePage：从 PdfReader 的 drag 事件带页码——通过自定义 dataTransfer 类型
s2 = s2.replace("""const readerRef = ref<InstanceType<typeof PdfReader> | null>(null)""",
"""const readerRef = ref<InstanceType<typeof PdfReader> | null>(null)
// 最近一次拖选的页码（PdfReader dragstart 写入 dataTransfer 'app/page'）
const lastQuotePage = ref<number | null>(null)""")

# onNoteDrop 里读页码
s2 = s2.replace("""  const paperTitle = paper.value?.title || '未知文献'""",
"""  const page = Number(e.dataTransfer?.getData('app/page') || 0)
  lastQuotePage.value = page || null
  const paperTitle = paper.value?.title || '未知文献'""")

# 样式：drop zone 高亮
s2 = s2.replace("""/* 笔记与编辑 */""",
""".note-drop-zone {
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

/* 笔记与编辑 */""")

io.open(p2, "w", encoding="utf-8", newline="\n").write(s2)

# ================= PdfReader dragstart 带页码 =================
p3 = "src/components/pdf/PdfReader.vue"
s3 = io.open(p3, encoding="utf-8").read()
s3 = s3.replace("""  e.dataTransfer?.setData('text/plain', dragPayload.value.quote)""",
"""  e.dataTransfer?.setData('text/plain', dragPayload.value.quote)
  e.dataTransfer?.setData('app/page', String(dragPayload.value.page))""")
io.open(p3, "w", encoding="utf-8", newline="\n").write(s3)

print("ui ok")
