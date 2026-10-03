// IPC 适配器：同一份前端代码在两种环境下运行——
//  - Tauri 桌面模式：走 IPC 命令（@tauri-apps/api）
//  - 浏览器/开发模式：走 dev-server 的 HTTP API（/api 前缀）
// 所有视图只 import 这里的 api 对象，不直接触碰传输层。

export interface AnalysisMeta {
  schema_version: number | null
  title: string
  authors: string[]
  year: number | null
  doi: string
  source_pdf: string
  tags: string[]
  extra: Record<string, string>
}

export interface SectionInfo {
  title: string
  content: string
  fixed: boolean
}

export interface PageLink {
  page: number
  context: string
}

export interface ParsedAnalysis {
  meta: AnalysisMeta
  has_front_matter: boolean
  sections: SectionInfo[]
  /** 首个二级栏目之前的正文（含一级标题行） */
  preamble: string
  missing_fixed: string[]
  duplicate_fixed: string[]
  warnings: string[]
  page_links: PageLink[]
  parse_status: 'ok' | 'partial' | 'unparsed'
}

export interface PaperListItem {
  id: string
  title: string
  authors: string[]
  year: number | null
  doi: string
  reading_status: 'to_read' | 'reading' | 'finished'
  analysis_status: 'none' | 'comparable' | 'needs_formatting'
  analysis_updated_at: number | null
  has_notes: boolean
  tags: string[]
  projects: string[]
  pdf_page_count: number | null
  trashed: boolean
  matched_on?: string[]
  created_at: number
  updated_at: number
}

export interface PaperDetail extends PaperListItem {
  pdf_size: number
  pdf_sha256: string
  last_page: number
  last_zoom: number
  last_mode: string
}

export interface ListQuery {
  query?: string
  project?: string
  tag?: string
  status?: string
  analysis?: string
  year?: number | null
  trash?: boolean
}

export interface ImportPreview {
  temp_token: string
  pdf: { sha256: string; size: number; page_count: number | null; page_count_error: string | null } | null
  md: ParsedAnalysis | null
  suggested_metadata: {
    title: string
    authors: string[]
    year: number | null
    doi: string
    title_source: string
  } | null
  page_warnings: string[]
  duplicates: { paper_id: string; title: string; reason: string }[]
  pairing_hint: string | null
}

export interface CommitRequest {
  temp_token?: string | null
  md_text?: string | null
  title: string
  authors: string[]
  year?: number | null
  doi: string
  tags: string[]
  project?: string | null
  update_paper_id?: string | null
  replace_pdf?: boolean
}

export interface CommitResult {
  paper: PaperDetail
  warnings: string[]
}

export interface AnalysisResponse {
  paper_id: string
  md_content: string
  prev_md_content: string | null
  parsed: ParsedAnalysis
  parse_status: string
  updated_at: number
}

export interface EvidenceRecord {
  id: number
  paper_id: string
  anchor_hash: string
  section: string
  excerpt: string
  page: number | null
  check_status: 'unverified' | 'verified'
  stale: boolean
  updated_at: number
}

export interface Synthesis {
  agreement: string
  differences: string
  gap: string
  conclusion: string
}

export interface CompareRecord {
  id: string
  name: string
  paper_ids: string[]
  dimensions: string[]
  synthesis: Synthesis
  snapshots: Record<string, number>
  created_at: number
  updated_at: number
}

export interface CompareWithStatus extends CompareRecord {
  updated_papers: string[]
}

export interface AiConfig {
  base_url: string
  api_key: string
  model: string
  max_context_tokens: number
  temperature: number
}

export interface AnnotationRect {
  x: number
  y: number
  w: number
  h: number
}

export interface PdfAnnotation {
  id: number
  paper_id: string
  page: number
  kind: 'highlight' | 'rect' | 'note'
  rects: AnnotationRect[]
  color: 'yellow' | 'red' | 'blue' | 'green'
  text: string
  quote: string
  created_at: number
  updated_at: number
}

export interface AnnotationInput {
  page: number
  kind: 'highlight' | 'rect' | 'note'
  rects: AnnotationRect[]
  color?: string
  text?: string
  quote?: string
}

export interface ProjectInfo { id: number; name: string; paper_count: number }
export interface TagInfo { id: number; name: string; paper_count: number }
export interface BackupResult { zip_path: string; papers: number; bytes: number }
export interface RestoreResult { library_dir: string; papers: number; warnings: string[] }

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** 常驻隐藏文件输入框（复用；也便于自动化测试注入文件）。 */
function hiddenInput(multiple: boolean): HTMLInputElement {
  let el = document.getElementById('litreview-file-input') as HTMLInputElement | null
  if (!el) {
    el = document.createElement('input')
    el.id = 'litreview-file-input'
    el.type = 'file'
    el.style.display = 'none'
    document.body.appendChild(el)
  }
  el.multiple = multiple
  el.value = ''
  return el
}

function waitInput(input: HTMLInputElement, multiple: boolean): Promise<{ files?: File[] } | null> {
  return new Promise((resolve) => {
    input.onchange = () => {
      const files = input.files ? Array.from(input.files) : []
      resolve(files.length ? (multiple ? { files } : { files: files.slice(0, 1) }) : null)
      input.onchange = null
    }
  })
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

async function httpJson<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined
  })
  const data = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error((data as { error?: string }).error || `请求失败 ${res.status}`)
  }
  return data as T
}

function qs(query: Record<string, unknown>): string {
  const p = new URLSearchParams()
  for (const [k, v] of Object.entries(query)) {
    if (v !== undefined && v !== null && v !== '') p.set(k, String(v))
  }
  const s = p.toString()
  return s ? `?${s}` : ''
}

export const api = {
  isTauri,

  listPapers(q: ListQuery = {}): Promise<PaperListItem[]> {
    if (isTauri) return tauriInvoke('list_papers', { query: q })
    return httpJson('GET', `/api/papers${qs(q as Record<string, unknown>)}`)
  },

  getPaper(id: string): Promise<PaperDetail> {
    if (isTauri) return tauriInvoke('get_paper', { id })
    return httpJson('GET', `/api/papers/${id}`)
  },

  patchPaper(id: string, patch: Record<string, unknown>): Promise<PaperDetail> {
    if (isTauri) return tauriInvoke('patch_paper', { id, patch })
    return httpJson('PATCH', `/api/papers/${id}`, patch)
  },

  deletePaper(id: string, permanent: boolean): Promise<void> {
    if (isTauri) return tauriInvoke('delete_paper', { id, permanent })
    return httpJson('DELETE', `/api/papers/${id}${qs({ mode: permanent ? 'permanent' : 'trash' })}`)
  },

  restorePaper(id: string): Promise<void> {
    if (isTauri) return tauriInvoke('restore_paper', { id })
    return httpJson('POST', `/api/papers/${id}/restore`, {})
  },

  async getPdfBytes(id: string): Promise<Uint8Array> {
    if (isTauri) {
      return tauriInvoke<number[]>('get_pdf', { id }).then((a) => new Uint8Array(a))
    }
    const res = await fetch(`/api/papers/${id}/pdf`)
    if (!res.ok) throw new Error(`获取 PDF 失败 ${res.status}`)
    return new Uint8Array(await res.arrayBuffer())
  },

  getPdfUrl(id: string): string {
    // 仅浏览器模式使用（<iframe>/直接打开）；Tauri 模式请用 getPdfBytes
    return `/api/papers/${id}/pdf`
  },

  getAnalysis(id: string): Promise<AnalysisResponse | null> {
    if (isTauri) return tauriInvoke('get_analysis', { id })
    return httpJson('GET', `/api/papers/${id}/analysis`)
  },

  setAnalysis(id: string, md: string): Promise<AnalysisResponse> {
    if (isTauri) return tauriInvoke('set_analysis', { id, md })
    return httpJson('PUT', `/api/papers/${id}/analysis`, { md })
  },

  restorePrevAnalysis(id: string): Promise<AnalysisResponse> {
    if (isTauri) return tauriInvoke('restore_prev_analysis', { id })
    return httpJson('POST', `/api/papers/${id}/analysis/restore-prev`, {})
  },

  getNote(id: string): Promise<{ paper_id: string; content: string; updated_at: number }> {
    if (isTauri) return tauriInvoke('get_note', { id })
    return httpJson('GET', `/api/papers/${id}/notes`)
  },

  setNote(id: string, content: string): Promise<{ updated_at: number }> {
    if (isTauri) return tauriInvoke('set_note', { id, content })
    return httpJson('PUT', `/api/papers/${id}/notes`, { content })
  },

  listAnnotations(id: string): Promise<PdfAnnotation[]> {
    if (isTauri) return tauriInvoke('list_annotations', { id })
    return httpJson('GET', `/api/papers/${id}/annotations`)
  },

  addAnnotation(id: string, input: AnnotationInput): Promise<PdfAnnotation> {
    if (isTauri) return tauriInvoke('add_annotation', { id, input })
    return httpJson('POST', `/api/papers/${id}/annotations`, input)
  },

  updateAnnotation(aid: number, patch: { color?: string; text?: string }): Promise<PdfAnnotation> {
    if (isTauri) return tauriInvoke('update_annotation', { aid, patch })
    return httpJson('PATCH', `/api/annotations/${aid}`, patch)
  },

  deleteAnnotation(aid: number): Promise<void> {
    if (isTauri) return tauriInvoke('delete_annotation', { aid })
    return httpJson('DELETE', `/api/annotations/${aid}`)
  },

  listEvidence(id: string): Promise<EvidenceRecord[]> {
    if (isTauri) return tauriInvoke('list_evidence', { id })
    return httpJson('GET', `/api/papers/${id}/evidence`)
  },

  upsertEvidence(
    id: string,
    input: { anchor_hash: string; section: string; excerpt: string; page: number | null; check_status: string }
  ): Promise<EvidenceRecord> {
    if (isTauri) return tauriInvoke('upsert_evidence', { id, input })
    return httpJson('PUT', `/api/papers/${id}/evidence`, input)
  },

  async exportPaper(id: string, includeNotes: boolean): Promise<{ filename: string; content: string }> {
    if (isTauri) return tauriInvoke('export_paper', { id, includeNotes })
    return httpJson('GET', `/api/papers/${id}/export${qs({ notes: includeNotes })}`)
  },

  /** 批量导出多篇文献为一份合并 Markdown。ids 为空数组时导出全部。 */
  async exportPapersBatch(ids: string[]): Promise<{ filename: string; content: string }> {
    if (isTauri) return tauriInvoke('export_papers_batch', { ids })
    return httpJson('GET', `/api/export-papers${qs({ ids: ids.join(',') })}`)
  },

  /** 纯笔记汇总导出（题录+个人笔记，不含 AI 分析；适合交给 AI 生成 PPT 大纲）。ids 为空数组时导出全部有笔记的。 */
  async exportNotesBatch(ids: string[]): Promise<{ filename: string; content: string }> {
    if (isTauri) return tauriInvoke('export_notes_batch', { ids })
    return httpJson('GET', `/api/export-notes${qs({ ids: ids.join(',') })}`)
  },

  /** 分析导入第一步。桌面模式传路径；浏览器模式传 File 对象或粘贴文本。 */
  async analyzeImport(input: {
    pdfPath?: string | null
    pdfFile?: File | null
    mdPath?: string | null
    mdFile?: File | null
    mdText?: string | null
  }): Promise<ImportPreview> {
    if (isTauri) {
      return tauriInvoke('analyze_import_paths', {
        pdfPath: input.pdfPath ?? null,
        mdPath: input.mdPath ?? null,
        mdText: input.mdText ?? null
      })
    }
    const fd = new FormData()
    if (input.pdfFile) fd.append('pdf', input.pdfFile)
    if (input.mdFile) fd.append('md', input.mdFile)
    if (input.mdText) fd.append('md_text', new Blob([input.mdText], { type: 'text/markdown' }))
    const res = await fetch('/api/import/analyze', { method: 'POST', body: fd })
    const data = await res.json()
    if (!res.ok) throw new Error(data.error || '解析失败')
    return data as ImportPreview
  },

  commitImport(req: CommitRequest): Promise<CommitResult> {
    if (isTauri) return tauriInvoke('commit_import', { req })
    return httpJson('POST', '/api/import/commit', req)
  },

  listProjects(): Promise<ProjectInfo[]> {
    if (isTauri) return tauriInvoke('list_projects')
    return httpJson('GET', '/api/projects')
  },

  createProject(name: string): Promise<ProjectInfo> {
    if (isTauri) return tauriInvoke('create_project', { name })
    return httpJson('POST', '/api/projects', { name })
  },

  renameProject(id: number, name: string): Promise<void> {
    if (isTauri) return tauriInvoke('rename_project', { id, name })
    return httpJson('PATCH', `/api/projects/${id}`, { name })
  },

  deleteProject(id: number): Promise<void> {
    if (isTauri) return tauriInvoke('delete_project', { id })
    return httpJson('DELETE', `/api/projects/${id}`)
  },

  listTags(): Promise<TagInfo[]> {
    if (isTauri) return tauriInvoke('list_tags')
    return httpJson('GET', '/api/tags')
  },

  deleteTag(id: number): Promise<void> {
    if (isTauri) return tauriInvoke('delete_tag', { id })
    return httpJson('DELETE', `/api/tags/${id}`)
  },

  listCompares(): Promise<CompareRecord[]> {
    if (isTauri) return tauriInvoke('list_compares')
    return httpJson('GET', '/api/compares')
  },

  createCompare(input: { name: string; paper_ids: string[]; dimensions?: string[] }): Promise<CompareRecord> {
    if (isTauri) return tauriInvoke('create_compare', { input })
    return httpJson('POST', '/api/compares', input)
  },

  getCompare(id: string): Promise<CompareWithStatus> {
    if (isTauri) return tauriInvoke('get_compare', { id })
    return httpJson('GET', `/api/compares/${id}`)
  },

  updateCompare(id: string, input: { name: string; paper_ids: string[]; dimensions: string[]; synthesis: Synthesis }): Promise<CompareRecord> {
    if (isTauri) return tauriInvoke('update_compare', { id, input })
    return httpJson('PUT', `/api/compares/${id}`, input)
  },

  deleteCompare(id: string): Promise<void> {
    if (isTauri) return tauriInvoke('delete_compare', { id })
    return httpJson('DELETE', `/api/compares/${id}`)
  },

  exportCompare(id: string): Promise<{ filename: string; content: string }> {
    if (isTauri) return tauriInvoke('export_compare', { id })
    return httpJson('GET', `/api/compares/${id}/export`)
  },

  backup(targetDir: string): Promise<BackupResult> {
    if (isTauri) return tauriInvoke('backup', { targetDir })
    return httpJson('POST', '/api/backup', { path: targetDir })
  },

  restore(zipPath: string, targetDir?: string | null): Promise<RestoreResult> {
    if (isTauri) return tauriInvoke('restore', { zipPath, targetDir: targetDir ?? null })
    return httpJson('POST', '/api/restore', { zip_path: zipPath, target_dir: targetDir ?? null })
  },

  getSettings(): Promise<{ library_dir: string }> {
    if (isTauri) return tauriInvoke('get_settings')
    return httpJson('GET', '/api/settings')
  },

  switchLibrary(dir: string): Promise<{ library_dir: string }> {
    if (isTauri) return tauriInvoke('switch_library', { libraryDir: dir })
    return httpJson('PUT', '/api/settings', { library_dir: dir })
  },

  /** 选择文件（PDF/MD）。桌面模式返回路径；浏览器模式返回 File。 */
  async pickFile(extensions?: string[]): Promise<{ path?: string; file?: File } | null> {
    if (isTauri) {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const path = await open({
        multiple: false,
        filters: extensions ? [{ name: '文件', extensions }] : undefined
      })
      if (typeof path === 'string') return { path }
      return null
    }
    const input = hiddenInput(false)
    if (extensions) input.accept = extensions.map((e) => `.${e}`).join(',')
    const p = waitInput(input, false)
    input.click()
    const r = await p
    return r?.files?.[0] ? { file: r.files[0] } : null
  },

  /** 多选文件（批量导入用）。桌面模式返回路径数组；浏览器模式返回 File 数组。 */
  async pickFiles(extensions?: string[]): Promise<{ paths?: string[]; files?: File[] } | null> {
    if (isTauri) {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const res = await open({
        multiple: true,
        filters: extensions ? [{ name: '文件', extensions }] : undefined
      })
      if (Array.isArray(res)) return { paths: res }
      if (typeof res === 'string') return { paths: [res] }
      return null
    }
    const input = hiddenInput(true)
    if (extensions) input.accept = extensions.map((e) => `.${e}`).join(',')
    const p = waitInput(input, true)
    input.click()
    const r = await p
    return r?.files?.length ? { files: r.files } : null
  },

  /** 选择一个 MD 文件并读出文本内容（补录分析用）。取消返回 null。 */
  async pickMdText(): Promise<string | null> {
    const r = await this.pickFile(['md', 'markdown', 'txt'])
    if (!r) return null
    if (r.file) return r.file.text()
    if (r.path) {
      if (isTauri) return tauriInvoke<string>('read_text_file', { path: r.path })
      return null
    }
    return null
  },

  /** 选择目录。桌面模式返回路径；浏览器模式返回 null（由用户手动输入路径）。 */
  async pickDir(): Promise<string | null> {
    if (isTauri) {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const dir = await open({ directory: true, multiple: false })
      return typeof dir === 'string' ? dir : null
    }
    return null
  }
}
