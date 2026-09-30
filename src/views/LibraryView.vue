<script setup lang="ts">
// 文献库主页：侧栏（导航/项目/标签/对比集合/底部工具）+ 搜索筛选 + 文献列表。
// 导入与设置均为 Dialog；支持批量导出（所选/全部）。
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import {
  ArrowDown,
  DataAnalysis,
  Notebook,
  Delete,
  Download,
  Edit,
  Files,
  FolderOpened,
  Histogram,
  Loading,
  MoreFilled,
  Plus,
  Promotion,
  Search,
  Setting
} from '@element-plus/icons-vue'
import { api, type ListQuery, type PaperListItem } from '../ipc'
import { useLibraryStore } from '../stores/library'
import { confirmAction, promptText, toastError, toastOk } from '../lib/toast'
import { clearBasket, getBasket } from '../lib/basket'
import ImportDialog from '../components/ImportDialog.vue'
import SettingsDialog from '../components/SettingsDialog.vue'
import logoUrl from '../assets/logo.png'

const router = useRouter()
const store = useLibraryStore()

const papers = ref<PaperListItem[]>([])
const loading = ref(false)
const loadedOnce = ref(false)

const search = ref('')
const filterProject = ref('')
const filterTag = ref('')
const filterStatus = ref('')
const filterAnalysis = ref('')
const inTrash = ref(false)
const selected = ref<Set<string>>(new Set())

const showImport = ref(false)
const showSettings = ref(false)

// 归入项目对话框（支持单篇/多篇）
const showAssign = ref(false)
const assignIds = ref<string[]>([])
const assignProjects = ref<string[]>([])
const assignSaving = ref(false)

function openAssign(ids: string[]) {
  if (ids.length === 0) return
  assignIds.value = ids
  // 预填第一篇当前所属项目（多篇时作为默认起点）
  const first = papers.value.find((p) => p.id === ids[0])
  assignProjects.value = first ? [...first.projects] : []
  showAssign.value = true
}

async function confirmAssign() {
  if (assignSaving.value) return
  assignSaving.value = true
  try {
    for (const id of assignIds.value) {
      await api.patchPaper(id, { projects: assignProjects.value })
    }
    toastOk(`已将 ${assignIds.value.length} 篇文献归入：${assignProjects.value.join('、') || '（未分类）'}`)
    showAssign.value = false
    await refresh()
    await store.refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    assignSaving.value = false
  }
}

const STATUS_LABELS: Record<string, string> = {
  to_read: '待阅读',
  reading: '阅读中',
  finished: '已完成'
}
const STATUS_TYPE: Record<string, 'info' | 'warning' | 'success'> = {
  to_read: 'info',
  reading: 'warning',
  finished: 'success'
}
const ANALYSIS_LABELS: Record<string, string> = {
  none: '未导入',
  comparable: '可对比',
  needs_formatting: '格式待整理'
}
const MATCH_LABELS: Record<string, string> = {
  title: '标题',
  authors: '作者',
  doi: 'DOI',
  tag: '标签',
  analysis: '分析',
  notes: '笔记'
}

async function refresh() {
  loading.value = true
  try {
    const q: ListQuery = {
      query: search.value.trim(),
      project: filterProject.value,
      tag: filterTag.value,
      status: filterStatus.value,
      analysis: filterAnalysis.value,
      trash: inTrash.value
    }
    papers.value = await api.listPapers(q)
    loadedOnce.value = true
  } catch (e) {
    toastError(String((e as Error).message || e))
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await store.ensureLoaded().catch(() => undefined)
  for (const id of getBasket()) selected.value.add(id)
  await refresh()
})

let searchTimer: ReturnType<typeof setTimeout> | null = null
watch(search, () => {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(refresh, 400)
})

function clearFilters() {
  filterProject.value = ''
  filterTag.value = ''
  filterStatus.value = ''
  filterAnalysis.value = ''
  search.value = ''
  refresh()
}

const hasFilter = computed(
  () => Boolean(search.value || filterProject.value || filterTag.value || filterStatus.value || filterAnalysis.value)
)

function setProject(name: string) {
  filterProject.value = filterProject.value === name ? '' : name
  refresh()
}

function setTag(name: string) {
  filterTag.value = filterTag.value === name ? '' : name
  refresh()
}

function setTrash(v: boolean) {
  inTrash.value = v
  selected.value = new Set()
  refresh()
}

function toggleSelect(id: string) {
  if (selected.value.has(id)) {
    selected.value.delete(id)
  } else {
    if (selected.value.size >= 5) {
      ElMessage.warning('一次对比最多选择 5 篇文献')
      return
    }
    selected.value.add(id)
  }
  selected.value = new Set(selected.value)
}

const compareDisabled = computed(() => selected.value.size < 2)

function compareSelected() {
  if (compareDisabled.value) return
  clearBasket()
  const ids = Array.from(selected.value).join(',')
  router.push({ path: '/compare/new', query: { p: ids } })
}

function readPaper(id: string) {
  router.push(`/read/${id}`)
}

// ---------- 批量导出 ----------
async function exportPapers(ids: string[], label: string) {
  try {
    const r = await api.exportPapersBatch(ids)
    await api.saveText(r.filename, r.content)
    toastOk(`已导出 ${label}（${r.filename}）`)
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

function exportSelected() {
  if (selected.value.size === 0) return
  exportPapers(Array.from(selected.value), `所选 ${selected.value.size} 篇`)
}

function exportAll() {
  if (papers.value.length === 0) {
    ElMessage.warning('当前列表没有文献')
    return
  }
  exportPapers([], '全部文献')
}

async function exportNotes(ids: string[], label: string) {
  try {
    const r = await api.exportNotesBatch(ids)
    await api.saveText(r.filename, r.content)
    toastOk(`已导出${label}（${r.filename}）`)
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

function exportAllNotes() {
  exportNotes([], '全部笔记')
}

function exportSelectedNotes() {
  if (selected.value.size === 0) return
  exportNotes(Array.from(selected.value), `所选 ${selected.value.size} 篇的笔记`)
}

// ---------- 行操作 ----------
async function moveToTrash(id: string, title: string) {
  const ok = await confirmAction(
    `将「${title}」移入回收站？文献数据与 PDF 会保留，可随时恢复。`,
    '移入回收站',
    '移入'
  )
  if (!ok) return
  try {
    await api.deletePaper(id, false)
    toastOk('已移入回收站')
    await refresh()
    await store.refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function restoreFromTrash(id: string) {
  try {
    await api.restorePaper(id)
    toastOk('已恢复')
    await refresh()
    await store.refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function deletePermanent(id: string, title: string) {
  const ok = await confirmAction(
    `永久删除「${title}」？\n将删除托管 PDF 副本与全部数据（笔记、分析、对比关联），不可恢复；您的原始文件不受影响。`,
    '永久删除',
    '永久删除'
  )
  if (!ok) return
  try {
    await api.deletePaper(id, true)
    toastOk('已永久删除')
    await refresh()
    await store.refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

/** 列表内直接为文献导入 MD 分析文件：选文件 → 解析入库，一步完成。 */
async function importMdFor(id: string) {
  let text: string | null = null
  try {
    text = await api.pickMdText()
  } catch (e) {
    toastError(String((e as Error).message || e))
    return
  }
  if (!text || !text.trim()) return
  try {
    const r = await api.setAnalysis(id, text)
    if (r.parsed.parse_status === 'unparsed') {
      ElMessage.warning('已导入，但未识别模板结构（分析状态：格式待整理）。可在阅读页「编辑 MD」整理后保存')
    } else if (r.parsed.missing_fixed.length) {
      toastOk(`MD 分析已导入；缺 ${r.parsed.missing_fixed.length} 个栏目（${r.parsed.missing_fixed.join('、')}，显示待补充）`)
    } else {
      toastOk('MD 分析已导入')
    }
    if (r.parsed.warnings.length) {
      ElMessage.info(r.parsed.warnings[0])
    }
    await refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function exportOne(id: string) {
  try {
    const r = await api.exportPaper(id, true)
    await api.saveText(r.filename, r.content)
    toastOk('已导出 Markdown')
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

// ---------- 项目管理 ----------
async function createProject() {
  const name = await promptText('新建项目', '输入项目名称，例如：课题一')
  if (!name) return
  try {
    await api.createProject(name)
    toastOk(`项目「${name}」已创建，导入文献时可选它归类`)
    await store.refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function renameProject(id: number, name: string) {
  const next = await promptText('重命名项目', '输入新名称', name)
  if (!next || next === name) return
  try {
    await api.renameProject(id, next)
    if (filterProject.value === name) filterProject.value = next
    toastOk('已重命名')
    await store.refresh()
    await refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function deleteProject(id: number, name: string) {
  const ok = await confirmAction(
    `删除项目「${name}」？仅取消文献归类，不会删除任何文献与数据。`,
    '删除项目',
    '删除'
  )
  if (!ok) return
  try {
    await api.deleteProject(id)
    if (filterProject.value === name) filterProject.value = ''
    toastOk('项目已删除')
    await store.refresh()
    await refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

// ---------- 对比集合 ----------
async function createCompare() {
  if (selected.value.size < 2) {
    ElMessage.warning('请先在列表中勾选 2～5 篇文献，再新建对比')
    return
  }
  compareSelected()
}

async function deleteCompare(id: string, name: string) {
  const ok = await confirmAction(`删除对比「${name}」？文献本身不受影响。`, '删除对比', '删除')
  if (!ok) return
  try {
    await api.deleteCompare(id)
    toastOk('对比已删除')
    await store.refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}

async function removeTag(id: number, name: string) {
  try {
    await api.deleteTag(id)
    if (filterTag.value === name) filterTag.value = ''
    toastOk(`标签「${name}」已删除`)
    await store.refresh()
    await refresh()
  } catch (e) {
    toastError(String((e as Error).message || e))
  }
}
</script>

<template>
  <el-container class="library-page">
    <!-- 侧栏 -->
    <el-aside class="sidebar">
      <div class="brand">
        <div class="brand-logo"><img :src="logoUrl" alt="文献综述中心" /></div>
        <div class="brand-text">
          <div class="brand-name">文献综述中心</div>
          <div class="brand-sub">本地文献工作台</div>
        </div>
      </div>

      <div class="side-scroll">
        <div class="side-item nav-item" :class="{ active: !inTrash && !filterProject && !filterTag }" @click="setTrash(false); clearFilters()">
          <el-icon><Files /></el-icon>
          <span class="side-item-name">全部文献</span>
          <span class="side-count">{{ papers.length }}</span>
        </div>

        <!-- 项目 -->
        <div class="side-group">
          <div class="side-group-head">
            <span class="side-group-title">项目</span>
            <el-tooltip content="新建项目" placement="right" :show-after="400">
              <button class="icon-mini" @click="createProject"><el-icon><Plus /></el-icon></button>
            </el-tooltip>
          </div>
          <transition-group name="list">
            <div v-for="p in store.projects" :key="p.id" class="side-item" :class="{ active: filterProject === p.name }">
              <button class="side-item-btn" @click="setProject(p.name)">
                <el-icon><FolderOpened /></el-icon>
                <span class="side-item-name">{{ p.name }}</span>
                <span class="side-count">{{ p.paper_count }}</span>
              </button>
              <el-dropdown trigger="click" @command="(cmd: string) => cmd === 'rename' ? renameProject(p.id, p.name) : deleteProject(p.id, p.name)">
                <button class="side-item-more" title="项目操作"><el-icon><MoreFilled /></el-icon></button>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item command="rename" :icon="Edit">重命名</el-dropdown-item>
                    <el-dropdown-item command="delete" :icon="Delete" divided>删除项目</el-dropdown-item>
                  </el-dropdown-menu>
                </template>
              </el-dropdown>
            </div>
          </transition-group>
          <div v-if="store.projects.length === 0" class="side-empty">
            还没有项目，点上方 <el-icon><Plus /></el-icon> 新建
          </div>
        </div>

        <!-- 标签 -->
        <div class="side-group">
          <div class="side-group-head">
            <span class="side-group-title">标签</span>
          </div>
          <div class="tag-cloud">
            <el-tag
              v-for="t in store.tags"
              :key="t.id"
              :effect="filterTag === t.name ? 'dark' : 'plain'"
              class="tag-chip"
              closable
              @click="setTag(t.name)"
              @close.stop="removeTag(t.id, t.name)"
            >{{ t.name }}</el-tag>
            <div v-if="store.tags.length === 0" class="side-empty">导入文献后自动生成</div>
          </div>
        </div>

        <!-- 已保存对比 -->
        <div class="side-group">
          <div class="side-group-head">
            <span class="side-group-title">已保存对比</span>
            <el-tooltip content="用已选文献新建对比" placement="right" :show-after="400">
              <button class="icon-mini" @click="createCompare"><el-icon><Plus /></el-icon></button>
            </el-tooltip>
          </div>
          <transition-group name="list">
            <div v-for="c in store.compares" :key="c.id" class="side-item">
              <button class="side-item-btn" @click="router.push(`/compare/${c.id}`)">
                <el-icon><DataAnalysis /></el-icon>
                <span class="side-item-name">{{ c.name }}</span>
                <span class="side-count">{{ c.paper_ids.length }}篇</span>
              </button>
              <button class="side-item-more" title="删除对比" @click="deleteCompare(c.id, c.name)">
                <el-icon><Delete /></el-icon>
              </button>
            </div>
          </transition-group>
          <div v-if="store.compares.length === 0" class="side-empty">勾选文献后可新建对比</div>
        </div>
      </div>

      <!-- 底部固定工具区 -->
      <div class="side-footer">
        <div class="side-item nav-item" :class="{ active: inTrash }" @click="setTrash(!inTrash)">
          <el-icon><Delete /></el-icon>
          <span class="side-item-name">回收站</span>
        </div>
        <div class="side-item nav-item" @click="showSettings = true">
          <el-icon><Setting /></el-icon>
          <span class="side-item-name">设置与备份</span>
        </div>
      </div>
    </el-aside>

    <!-- 主区 -->
    <el-container class="main-area">
      <!-- 自适应工具栏：宽屏铺满伸展，窄屏自动换行 -->
      <div class="lib-toolbar">
        <el-input
          v-model="search"
          class="search-input"
          placeholder="搜索标题 / 作者 / DOI / 标签 / 分析内容 / 笔记…"
          clearable
          :prefix-icon="Search"
          @clear="refresh"
          @keyup.enter="refresh"
        />
        <el-select v-model="filterProject" placeholder="项目" clearable filterable class="filter-select" @change="refresh">
          <el-option v-for="p in store.projects" :key="p.id" :label="p.name" :value="p.name" />
        </el-select>
        <el-select v-model="filterTag" placeholder="标签" clearable filterable class="filter-select" @change="refresh">
          <el-option v-for="t in store.tags" :key="t.id" :label="t.name" :value="t.name" />
        </el-select>
        <el-select v-model="filterStatus" placeholder="阅读状态" clearable class="filter-select" @change="refresh">
          <el-option label="待阅读" value="to_read" />
          <el-option label="阅读中" value="reading" />
          <el-option label="已完成" value="finished" />
        </el-select>
        <el-select v-model="filterAnalysis" placeholder="分析状态" clearable class="filter-select" @change="refresh">
          <el-option label="可对比" value="comparable" />
          <el-option label="格式待整理" value="needs_formatting" />
          <el-option label="未导入" value="none" />
        </el-select>
        <el-button v-if="hasFilter" text type="primary" class="toolbar-flex-item" @click="clearFilters">清除筛选</el-button>
        <span v-if="loading" class="loading-hint toolbar-flex-item"><el-icon class="is-loading"><Loading /></el-icon> 加载中…</span>
        <span class="toolbar-actions">
          <el-button :icon="Download" @click="exportAll">导出全部</el-button>
        <el-button :icon="Notebook" @click="exportAllNotes">导出笔记</el-button>
          <el-button type="primary" :icon="Plus" @click="showImport = true">导入文献</el-button>
        </span>
      </div>

      <!-- 列表 -->
      <el-main class="paper-list-wrap">
        <div v-if="loading && !loadedOnce" class="skeleton-area">
          <el-skeleton v-for="i in 4" :key="i" :rows="2" animated />
        </div>

        <el-empty v-else-if="papers.length === 0" :description="inTrash ? '回收站为空' : hasFilter ? '没有符合条件的文献' : '资料库还是空的'">
          <template v-if="!inTrash && !hasFilter">
            <el-button type="primary" :icon="Plus" @click="showImport = true">导入第一篇文献</el-button>
            <p class="empty-hint">支持批量：一次选择多个 PDF 与同名 MD 自动配对</p>
          </template>
          <el-button v-if="hasFilter" @click="clearFilters">清除筛选</el-button>
        </el-empty>

        <transition-group v-else name="list" tag="div" class="paper-list">
          <div v-for="p in papers" :key="p.id" class="paper-row" :class="{ selected: selected.has(p.id) }">
            <el-checkbox
              v-if="!inTrash"
              :model-value="selected.has(p.id)"
              class="row-check"
              @change="toggleSelect(p.id)"
            />
            <div class="paper-main" @click="readPaper(p.id)" @dblclick="readPaper(p.id)">
              <div class="paper-title-line">
                <span class="paper-title">{{ p.title }}</span>
                <span v-if="p.year" class="paper-year">{{ p.year }}</span>
              </div>
              <div class="paper-meta">
                <span v-if="p.authors.length" class="meta-authors">
                  {{ p.authors.slice(0, 3).join(', ') }}{{ p.authors.length > 3 ? ' 等' : '' }}
                </span>
                <el-tag size="small" :type="STATUS_TYPE[p.reading_status]" effect="light">
                  {{ STATUS_LABELS[p.reading_status] }}
                </el-tag>
                <el-tag
                  size="small"
                  :type="p.analysis_status === 'comparable' ? 'success' : 'info'"
                  :effect="p.analysis_status === 'none' ? 'light' : 'plain'"
                  :class="{ 'clickable-tag': p.analysis_status === 'none' }"
                  :title="p.analysis_status === 'none' ? '点击导入 MD 分析文件' : undefined"
                  @click.stop="p.analysis_status === 'none' && importMdFor(p.id)"
                >
                  {{ ANALYSIS_LABELS[p.analysis_status] }}
                </el-tag>
                <el-tag v-if="p.has_notes" size="small" type="warning" effect="plain">有笔记</el-tag>
                <el-tag
                  v-for="pr in p.projects.slice(0, 2)"
                  :key="pr"
                  size="small"
                  effect="dark"
                  type="primary"
                  class="meta-tag project-tag"
                  title="点击筛选该项目"
                  @click.stop="setProject(pr)"
                >{{ pr }}</el-tag>
                <el-tag v-for="t in p.tags.slice(0, 4)" :key="t" size="small" effect="plain" class="meta-tag">{{ t }}</el-tag>
                <span v-if="p.matched_on && p.matched_on.length" class="match-hint">
                  命中：{{ p.matched_on.map((m) => MATCH_LABELS[m] || m).join(' / ') }}
                </span>
              </div>
            </div>
            <div class="paper-actions" @click.stop>
              <template v-if="!inTrash">
                <el-button v-if="p.analysis_status === 'none'" text type="success" @click="importMdFor(p.id)">导入MD</el-button>
                <el-button text type="primary" @click="readPaper(p.id)">阅读</el-button>
                <el-dropdown trigger="click" @command="(cmd: string) => {
                  if (cmd === 'compare') { toggleSelect(p.id); ElMessage.info('已加入底部选择条，继续勾选后点击对比') }
                  else if (cmd === 'export') exportOne(p.id)
                  else if (cmd === 'assign') openAssign([p.id])
                  else if (cmd === 'importMd') importMdFor(p.id)
                  else if (cmd === 'editMd') router.push(`/read/${p.id}?tab=edit`)
                  else moveToTrash(p.id, p.title)
                }">
                  <el-button text><el-icon><MoreFilled /></el-icon></el-button>
                  <template #dropdown>
                    <el-dropdown-menu>
                      <el-dropdown-item command="importMd" :icon="Promotion">导入 MD 分析文件…</el-dropdown-item>
                      <el-dropdown-item command="editMd" :icon="Edit">编辑 MD 内容</el-dropdown-item>
                      <el-dropdown-item command="assign" :icon="FolderOpened">归入项目</el-dropdown-item>
                      <el-dropdown-item command="compare" :icon="Histogram">加入对比</el-dropdown-item>
                      <el-dropdown-item command="export" :icon="Download">导出 Markdown</el-dropdown-item>
                      <el-dropdown-item command="trash" :icon="Delete" divided>移入回收站</el-dropdown-item>
                    </el-dropdown-menu>
                  </template>
                </el-dropdown>
              </template>
              <template v-else>
                <el-button text type="primary" @click="restoreFromTrash(p.id)">恢复</el-button>
                <el-button text type="danger" @click="deletePermanent(p.id, p.title)">永久删除</el-button>
              </template>
            </div>
          </div>
        </transition-group>

        <!-- 底部选择条 -->
        <transition name="bar">
          <div v-if="!inTrash && selected.size > 0" class="select-bar">
            <span>
              已选 <strong>{{ selected.size }}</strong> 篇（2～5 篇可对比）
            </span>
            <span class="bar-actions">
              <el-button @click="selected = new Set()">取消选择</el-button>
              <el-button :icon="FolderOpened" @click="openAssign(Array.from(selected))">归入项目</el-button>
              <el-dropdown trigger="click" @command="(cmd: string) => cmd === 'notes' ? exportSelectedNotes() : exportSelected()">
                <el-button :icon="Download">导出所选<el-icon class="el-icon--right"><ArrowDown /></el-icon></el-button>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item command="full">完整内容（分析+笔记）</el-dropdown-item>
                    <el-dropdown-item command="notes">仅个人笔记（汇总）</el-dropdown-item>
                  </el-dropdown-menu>
                </template>
              </el-dropdown>
              <el-button type="primary" :disabled="compareDisabled" :icon="DataAnalysis" @click="compareSelected">
                对比所选文献
              </el-button>
            </span>
          </div>
        </transition>
      </el-main>
    </el-container>

    <!-- 导入与设置对话框 -->
    <ImportDialog v-model="showImport" @imported="refresh()" />
    <SettingsDialog v-model="showSettings" />

    <!-- 归入项目对话框 -->
    <el-dialog v-model="showAssign" title="归入项目" width="460px" :close-on-click-modal="false">
      <p class="assign-hint">
        为选中的 <strong>{{ assignIds.length }}</strong> 篇文献设置所属项目（一篇可属多个项目，不复制文件）。
        可选择已有项目，或直接输入新项目名。
      </p>
      <el-select
        v-model="assignProjects"
        multiple
        filterable
        allow-create
        default-first-option
        placeholder="选择或输入项目名"
        style="width: 100%"
      >
        <el-option v-for="pr in store.projects" :key="pr.id" :label="pr.name" :value="pr.name" />
      </el-select>
      <template #footer>
        <el-button @click="showAssign = false">取消</el-button>
        <el-button type="primary" :loading="assignSaving" @click="confirmAssign">保存</el-button>
      </template>
    </el-dialog>
  </el-container>
</template>

<style scoped>
.library-page {
  height: 100%;
  overflow: hidden;
}

/* ---------- 侧栏 ---------- */
.sidebar {
  width: 248px;
  border-right: 1px solid #e6e9ef;
  background: linear-gradient(180deg, #f7f9fc 0%, #f2f5f9 100%);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 16px;
  border-bottom: 1px solid #e6e9ef;
  flex: none;
}

.brand-logo {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  overflow: hidden;
  box-shadow: 0 2px 6px rgba(31, 45, 61, 0.18);
  flex: none;
  background: #fff;
}

.brand-logo img {
  width: 100%;
  height: 100%;
  display: block;
  object-fit: cover;
}

.brand-name {
  font-size: 15px;
  font-weight: 700;
  letter-spacing: 0.5px;
  color: #1f2329;
}

.brand-sub {
  font-size: 11px;
  color: #8f959e;
  margin-top: 1px;
}

.side-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 10px 10px 12px;
}

.side-item {
  position: relative;
  display: flex;
  align-items: center;
  border-radius: 8px;
  margin: 2px 0;
  transition: background 0.15s;
  color: #3b4048;
}

.side-item:hover {
  background: var(--el-color-primary-light-9);
}

.nav-item {
  padding: 9px 12px;
  gap: 9px;
  font-size: 13.5px;
  cursor: pointer;
  user-select: none;
}

.nav-item.active {
  background: #fff;
  color: var(--el-color-primary);
  font-weight: 600;
  box-shadow: 0 1px 4px rgba(31, 45, 61, 0.08);
}

.nav-item.active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 20%;
  bottom: 20%;
  width: 3px;
  border-radius: 2px;
  background: var(--el-color-primary);
}

.side-group {
  margin-top: 14px;
  padding: 0 4px;
}

.side-group-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 8px 4px;
}

.side-group-title {
  font-size: 11.5px;
  color: #8f959e;
  font-weight: 600;
  letter-spacing: 1px;
}

.icon-mini {
  border: none;
  background: none;
  padding: 3px;
  border-radius: 5px;
  color: #8f959e;
  cursor: pointer;
  display: flex;
  align-items: center;
  transition: all 0.15s;
}

.icon-mini:hover {
  color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}

.side-item-btn {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border: none;
  background: none;
  text-align: left;
  font-size: 13px;
  color: inherit;
  cursor: pointer;
  border-radius: 8px;
}

.side-item.active {
  background: #fff;
  color: var(--el-color-primary);
  box-shadow: 0 1px 4px rgba(31, 45, 61, 0.08);
}

.side-item.active .side-item-btn {
  font-weight: 600;
}

.side-item-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.side-count {
  flex: none;
  font-size: 11.5px;
  color: #8f959e;
  background: rgba(143, 149, 158, 0.12);
  border-radius: 8px;
  padding: 0 7px;
  line-height: 17px;
}

.side-item.active .side-count {
  background: var(--el-color-primary-light-8);
  color: var(--el-color-primary);
}

.side-item-more {
  flex: none;
  border: none;
  background: none;
  padding: 4px 6px;
  color: #8f959e;
  cursor: pointer;
  border-radius: 5px;
  opacity: 0;
  transition: opacity 0.15s;
}

.side-item:hover .side-item-more {
  opacity: 1;
}

.side-item-more:hover {
  color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}

.side-empty {
  padding: 4px 8px;
  font-size: 12px;
  color: #a8abb2;
  display: flex;
  align-items: center;
  gap: 4px;
}

.tag-cloud {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 4px 6px;
}

.tag-chip {
  cursor: pointer;
  transition: transform 0.15s;
}

.tag-chip:hover {
  transform: translateY(-1px);
}

/* 底部固定区 */
.side-footer {
  flex: none;
  border-top: 1px solid #e6e9ef;
  padding: 8px 10px;
  background: rgba(255, 255, 255, 0.6);
}

/* ---------- 主区 ---------- */
.main-area {
  min-width: 0;
  /* el-header 换为 div 后 el-container 会误判为横向布局，显式纵向排列 */
  flex-direction: column;
  display: flex;
}

.lib-toolbar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 20px;
  background: #fff;
  border-bottom: 1px solid #e6e9ef;
  flex-wrap: wrap;
}

/* 搜索框弹性伸展：宽屏吃掉剩余空间，窄屏最小 420px 后换行 */
.search-input {
  flex: 2 1 460px;
  min-width: 320px;
  max-width: 900px;
}

/* 筛选下拉随容器伸展（自适应核心），窄屏最小 160px */
.filter-select {
  flex: 1 1 180px;
  min-width: 160px;
  max-width: 260px;
}

.toolbar-flex-item {
  flex: none;
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-left: auto;
}

/* 超宽屏（≥1800px）：侧栏与控件适度放大，列表限宽居中保证可读性 */
@media (min-width: 1800px) {
  .sidebar {
    width: 264px;
  }

  .paper-list,
  .skeleton-area {
    max-width: 1560px;
    margin: 0 auto;
    width: 100%;
  }
}

.loading-hint {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: #8f959e;
  font-size: 12px;
}

.paper-list-wrap {
  position: relative;
  overflow-y: auto;
  padding: 12px 20px 90px;
  display: flex;
  flex-direction: column;
}

/* 空状态在剩余空间内垂直居中 */
.paper-list-wrap > .el-empty {
  margin: auto;
}

.skeleton-area {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 20px;
  max-width: 760px;
}

.empty-hint {
  color: #8f959e;
  font-size: 12px;
  margin: 10px 0 0;
}

.paper-list {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.paper-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border: 1px solid #e6e9ef;
  border-radius: 8px;
  background: #fff;
  transition: border-color 0.15s, box-shadow 0.2s, transform 0.15s;
}

.paper-row:hover {
  border-color: var(--el-color-primary-light-5);
  box-shadow: 0 2px 12px var(--el-color-primary-light-9);
  transform: translateY(-1px);
}

.paper-row.selected {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}

.row-check {
  flex: none;
  height: auto;
}

.paper-main {
  flex: 1;
  min-width: 0;
  cursor: pointer;
}

.paper-title-line {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.paper-title {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.paper-row:hover .paper-title {
  color: var(--el-color-primary);
}

.paper-year {
  flex: none;
  color: #8f959e;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
}

.paper-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 5px;
  flex-wrap: wrap;
}

.meta-authors {
  color: #8f959e;
  font-size: 12px;
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meta-tag {
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.project-tag {
  cursor: pointer;
}

.clickable-tag {
  cursor: pointer;
}

.clickable-tag:hover {
  outline: 1px solid currentColor;
}

.assign-hint {
  color: #8f959e;
  font-size: 13px;
  line-height: 1.7;
  margin: 0 0 12px;
}

.match-hint {
  font-size: 12px;
  color: var(--el-color-primary);
}

.paper-actions {
  flex: none;
  display: flex;
  align-items: center;
  gap: 4px;
}

/* ---------- 选择条 ---------- */
.select-bar {
  position: absolute;
  left: 50%;
  bottom: 20px;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 20px;
  padding: 10px 20px;
  background: #24292f;
  color: #fff;
  border-radius: 999px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.28);
  z-index: 100;
  white-space: nowrap;
}

.bar-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.bar-enter-active,
.bar-leave-active {
  transition: all 0.25s ease;
}

.bar-enter-from,
.bar-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(20px);
}
</style>
