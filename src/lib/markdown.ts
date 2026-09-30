// Markdown 渲染管线：marked 解析 → DOMPurify 消毒 → KaTeX 公式回填。
// 安全约束（需求 8.3）：不执行脚本、不加载远程图片。
// 数学公式以占位符方式在消毒后回填，KaTeX 输出为本地生成的可信 HTML。

import { marked } from 'marked'
import DOMPurify from 'dompurify'
import katex from 'katex'

marked.setOptions({ gfm: true, breaks: false })

interface MathSlot {
  tex: string
  display: boolean
}

/** 从 Markdown 文本提取 $$...$$ 与 $...$（跳过代码围栏内的内容），替换为占位符。 */
function extractMath(src: string): { text: string; slots: MathSlot[] } {
  const slots: MathSlot[] = []
  const segments = src.split(/(```[\s\S]*?```|~~~[\s\S]*?~~~)/g)
  const text = segments
    .map((seg, i) => {
      // 偶数索引为普通文本段（split 带捕获组时，代码块位于奇数索引）
      if (i % 2 === 1) return seg
      return seg.replace(/\$\$([\s\S]+?)\$\$|\\\(([\s\S]+?)\\\)|\$([^\$\n]+?)\$|\\\[([\s\S]+?)\\\]/g,
        (m, dd, ip, inl, dt) => {
          const tex = dd ?? ip ?? inl ?? dt ?? ''
          const display = dd !== undefined || dt !== undefined
          slots.push({ tex, display })
          return `MATHZ${slots.length - 1}Z`
        })
    })
    .join('')
  return { text, slots }
}

export function renderMarkdown(md: string): string {
  if (!md) return ''
  const { text, slots } = extractMath(md)
  const rawHtml = marked.parse(text, { async: false }) as string
  const clean = DOMPurify.sanitize(rawHtml, {
    FORBID_TAGS: ['img', 'script', 'iframe', 'object', 'embed', 'style'],
    FORBID_ATTR: ['onerror', 'onload', 'onclick']
  })
  // 消毒后回填 KaTeX（占位符存活为纯文本；KaTeX 输出本地生成，可信）
  return clean.replace(/MATHZ(\d+)Z/g, (_, n: string) => {
    const slot = slots[Number(n)]
    if (!slot) return ''
    try {
      return katex.renderToString(slot.tex, {
        displayMode: slot.display,
        throwOnError: false,
        strict: false
      })
    } catch {
      return `<code>${escapeHtml(slot.tex)}</code>`
    }
  })
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/** 剥离 YAML 前置元数据块，仅保留正文（渲染分析时使用，避免泄漏元数据原文）。 */
export function stripFrontMatter(md: string): string {
  const text = md.replace(/^\uFEFF/, '')
  const lines = text.split('\n')
  if (lines[0]?.trim() !== '---') return text
  for (let i = 1; i < lines.length; i++) {
    const t = lines[i].trim()
    if (t === '---' || t === '...') {
      return lines.slice(i + 1).join('\n')
    }
  }
  return text
}

/** 把容器内的 #pdf-page-N 链接转换为可点击的页码跳转元素。 */
export function bindPageLinks(container: HTMLElement, onGoto: (page: number) => void): void {
  const links = container.querySelectorAll<HTMLAnchorElement>('a[href^="#pdf-page-"]')
  links.forEach((a) => {
    const m = a.getAttribute('href')?.match(/^#pdf-page-(\d+)$/)
    if (!m) return
    const page = Number(m[1])
    const span = document.createElement('span')
    span.className = 'page-link'
    span.textContent = a.textContent || `PDF 第 ${page} 页`
    span.title = `跳转到 PDF 第 ${page} 页`
    span.addEventListener('click', (e) => {
      e.stopPropagation()
      onGoto(page)
    })
    a.replaceWith(span)
  })
}

/** 计算文本锚点哈希（与后端证据记录对齐：同一段内容得到同一 anchor）。 */
export async function anchorHash(paperId: string, section: string, excerpt: string): Promise<string> {
  const data = new TextEncoder().encode(`${paperId}::${section}::${excerpt.trim().slice(0, 200)}`)
  const digest = await crypto.subtle.digest('SHA-256', data)
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}
