// 主题色系统：色值选自「中国色」（https://www.2kil.com/）传统色板。
// 运行时覆盖 Element Plus 主色变量（含 light/dark 派生档），localStorage 持久化。
export interface ThemePreset {
  id: string
  /** 中国色名 */
  name: string
  /** 心情/氛围 */
  mood: string
  color: string
  /** 情绪图标 key（见 moodIcons.ts） */
  icon: string
  /** 来源标注 */
  origin: string
}

export const THEMES: ThemePreset[] = [
  { id: 'default', name: '晴蓝', mood: '清爽 · 明快', color: '#409EFF', icon: 'sun', origin: '默认主题' },
  { id: 'zhuyue', name: '竹月', mood: '沉静 · 专注', color: '#2578B5', icon: 'moon', origin: '中国色 · 深竹月' },
  { id: 'chencheng', name: '晨橙', mood: '元气 · 活力', color: '#E67A2A', icon: 'rocket', origin: '中国色 · 橙色' },
  { id: 'haitang', name: '海棠', mood: '热情 · 明艳', color: '#D13559', icon: 'fireworks', origin: '中国色 · 银星海棠' },
  { id: 'hehua', name: '荷花', mood: '温柔 · 甜美', color: '#EB7FAF', icon: 'candy', origin: '中国色 · 荷花红' },
  { id: 'donglv', name: '冬绿', mood: '平静 · 舒缓', color: '#68927D', icon: 'leaf', origin: '中国色 · 冬绿' },
  { id: 'qinglian', name: '青莲', mood: '灵感 · 神秘', color: '#95509F', icon: 'magic', origin: '中国色 · 青莲紫' }
]

const STORAGE_KEY = 'litreview-theme'

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace('#', '')
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)]
}

function rgbToHex(r: number, g: number, b: number): string {
  const c = (n: number) => Math.round(n).toString(16).padStart(2, '0')
  return `#${c(r)}${c(g)}${c(b)}`
}

/** 颜色混合：weight 为混入 b 的比例（0~1）。 */
function mix(a: string, b: string, weight: number): string {
  const [r1, g1, b1] = hexToRgb(a)
  const [r2, g2, b2] = hexToRgb(b)
  return rgbToHex(r1 + (r2 - r1) * weight, g1 + (g2 - g1) * weight, b1 + (b2 - b1) * weight)
}

export function applyTheme(color: string): void {
  const root = document.documentElement
  const [r, g, b] = hexToRgb(color)
  root.style.setProperty('--el-color-primary', color)
  for (const i of [3, 5, 7, 8, 9]) {
    root.style.setProperty(`--el-color-primary-light-${i}`, mix(color, '#ffffff', i / 10))
  }
  root.style.setProperty('--el-color-primary-dark-2', mix(color, '#000000', 0.2))
  // 供自定义样式与阴影使用的变量
  root.style.setProperty('--primary', color)
  root.style.setProperty('--primary-rgb', `${r}, ${g}, ${b}`)
}

export function getCurrentThemeId(): string {
  return localStorage.getItem(STORAGE_KEY) || 'default'
}

export function initTheme(): void {
  const id = getCurrentThemeId()
  const preset = THEMES.find((t) => t.id === id) || THEMES[0]
  applyTheme(preset.color)
}

export function selectTheme(id: string): ThemePreset {
  const preset = THEMES.find((t) => t.id === id) || THEMES[0]
  localStorage.setItem(STORAGE_KEY, preset.id)
  applyTheme(preset.color)
  return preset
}
