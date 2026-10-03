# -*- coding: utf-8 -*-
"""批次3 模板补丁：侧栏三态/缩略图/大纲/搜索面板/搜索高亮"""
import io

p = "src/components/pdf/PdfReader.vue"
s = io.open(p, encoding="utf-8").read()

# ---------- 1) 工具栏：侧栏切换改三按钮 ----------
OLD_TOOLBAR_HEAD = """      <el-tooltip content="注释列表" placement="bottom">
        <el-button size="small" :type="sidebarOpen ? 'primary' : ''" :icon="Collection" @click="sidebarOpen = !sidebarOpen" />
      </el-tooltip>
      <el-divider direction="vertical" />"""
NEW_TOOLBAR_HEAD = """      <el-button-group size="small">
        <el-button size="small" :type="sidebarOpen && sidebarMode === 'annotations' ? 'primary' : ''" :icon="Collection" title="注释列表" @click="switchSidebar('annotations')" />
        <el-button size="small" :type="sidebarOpen && sidebarMode === 'thumbnails' ? 'primary' : ''" title="页面缩略图" @click="switchSidebar('thumbnails')">▤</el-button>
        <el-button size="small" :type="sidebarOpen && sidebarMode === 'outline' ? 'primary' : ''" title="文档大纲" @click="switchSidebar('outline')">☰</el-button>
      </el-button-group>
      <el-button size="small" title="查找 (Ctrl+F)" @click="searchOpen = true; focusSearchInput()">⌕</el-button>
      <el-divider direction="vertical" />"""
assert OLD_TOOLBAR_HEAD in s, "toolbar head missing"
s = s.replace(OLD_TOOLBAR_HEAD, NEW_TOOLBAR_HEAD, 1)

# ---------- 2) 侧栏内容：v-if 分三态 ----------
OLD_SIDEBAR = """      <!-- 常驻注释侧栏 -->
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
      </aside>"""
NEW_SIDEBAR = """      <!-- 侧栏（三态：注释 / 缩略图 / 大纲） -->
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
      </aside>"""
assert OLD_SIDEBAR in s, "sidebar block missing"
s = s.replace(OLD_SIDEBAR, NEW_SIDEBAR, 1)

# ---------- 3) 搜索高亮 overlay（页元素内，anno-svg 之后） ----------
OLD_BADGE = """          <span class="page-badge">{{ p.index }}</span>
        </div>"""
NEW_BADGE = """          <!-- 搜索命中高亮 -->
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
        </div>"""
assert OLD_BADGE in s, "badge anchor missing"
s = s.replace(OLD_BADGE, NEW_BADGE, 1)

# ---------- 4) 搜索面板（浮条前插入） ----------
OLD_POP = """        <!-- 选中浮条 -->"""
NEW_POP = """        <!-- Ctrl+F 搜索面板 -->
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

        <!-- 选中浮条 -->"""
assert OLD_POP in s, "pop anchor missing"
s = s.replace(OLD_POP, NEW_POP, 1)

# ---------- 5) 样式 ----------
OLD_CSS = """/* 选中浮条 */"""
NEW_CSS = """/* 缩略图 */
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

/* 选中浮条 */"""
assert OLD_CSS in s, "css anchor missing"
s = s.replace(OLD_CSS, NEW_CSS, 1)

io.open(p, "w", encoding="utf-8", newline="\n").write(s)
print("template part ok")
