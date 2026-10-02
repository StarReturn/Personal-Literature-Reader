export interface BibliographicMetadata {
  title: string
  authors: string[]
  year: number | null
  doi: string
}

/** 仅用 DOI 查询 Crossref；网络失败时调用方保留 PDF 本地识别结果。 */
export async function lookupDoiMetadata(rawDoi: string, timeoutMs = 4000): Promise<BibliographicMetadata | null> {
  const doi = rawDoi.trim().replace(/^(?:https?:\/\/)?(?:dx\.)?doi\.org\//i, '').replace(/^doi:\s*/i, '')
  if (!/^10\.\d{4,9}\/\S+$/i.test(doi)) return null
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetch(`https://api.crossref.org/works/${encodeURIComponent(doi)}`, {
      signal: controller.signal,
      headers: { Accept: 'application/json' }
    })
    if (response.status === 404) return null
    if (!response.ok) throw new Error(`Crossref HTTP ${response.status}`)
    const result = await response.json()
    const work = result?.message
    if (!work || typeof work !== 'object') return null
    const title = Array.isArray(work.title) && typeof work.title[0] === 'string' ? work.title[0].trim() : ''
    const authors = Array.isArray(work.author)
      ? work.author.map((person: { given?: string; family?: string; name?: string }) =>
          person.name || [person.given, person.family].filter(Boolean).join(' ')).filter(Boolean)
      : []
    const year = work.published?.['date-parts']?.[0]?.[0] ?? work.issued?.['date-parts']?.[0]?.[0] ?? null
    return {
      title,
      authors,
      year: typeof year === 'number' && year >= 1800 && year <= 2100 ? year : null,
      doi: typeof work.DOI === 'string' ? work.DOI : doi
    }
  } finally {
    clearTimeout(timeout)
  }
}
