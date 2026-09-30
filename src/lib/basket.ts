// 对比选择篮：文献库与阅读页共享的"加入对比"暂存（会话级）。
const KEY = 'litreview-compare-basket'

export function getBasket(): string[] {
  try {
    const v = JSON.parse(sessionStorage.getItem(KEY) || '[]')
    return Array.isArray(v) ? v : []
  } catch {
    return []
  }
}

export function addToBasket(id: string): { added: boolean; size: number } {
  const b = getBasket()
  if (b.includes(id)) return { added: false, size: b.length }
  if (b.length >= 5) return { added: false, size: b.length }
  b.push(id)
  sessionStorage.setItem(KEY, JSON.stringify(b))
  return { added: true, size: b.length }
}

export function removeFromBasket(id: string): void {
  const b = getBasket().filter((x) => x !== id)
  sessionStorage.setItem(KEY, JSON.stringify(b))
}

export function clearBasket(): void {
  sessionStorage.removeItem(KEY)
}
