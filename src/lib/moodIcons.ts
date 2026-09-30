// 情绪图标：取自 icon-park-outline（iconfont 风格，48x48 viewBox），
// 构建期从 @iconify-json/icon-park-outline 提取（值为完整 SVG 内部标记，自带描边属性），运行时零依赖。
export const MOOD_ICONS: Record<string, string> = {
  sun: '<g fill="none" stroke="currentColor" stroke-linejoin="round" stroke-width="4"><path stroke-linecap="round" d="m9.15 9.15l2.228 2.228M3 24h3.15m3 14.85l2.228-2.228M38.85 38.85l-2.228-2.228M45 24h-3.15m-3-14.85l-2.228 2.228M24 3v3.15"/><path fill="currentColor" d="M24 36c6.627 0 12-5.373 12-12s-5.373-12-12-12s-12 5.373-12 12s5.373 12 12 12Z"/><path stroke-linecap="round" d="M24 45v-3.15"/></g>',
  moon: '<path fill="none" stroke="currentColor" stroke-linejoin="round" stroke-width="4" d="M28.053 4.41c-5.47 1.427-9.507 6.4-9.507 12.317c0 7.03 5.698 12.728 12.727 12.728c5.916 0 10.89-4.038 12.316-9.508A20 20 0 0 1 44 24c0 11.046-8.954 20-20 20S4 35.046 4 24S12.954 4 24 4c1.389 0 2.744.141 4.053.41Z"/>',
  rocket: '<g fill="none" stroke="currentColor" stroke-width="4"><path stroke-linecap="round" stroke-linejoin="round" d="m20.906 6.063l1.43-.954a3 3 0 0 1 3.328 0l1.43.954A20 20 0 0 1 36 22.703V33H12V22.704a20 20 0 0 1 8.906-16.641"/><circle cx="24" cy="20" r="6"/><path stroke-linecap="round" stroke-linejoin="round" d="m12 22l-6 6.217V33h36v-4.783L36 22M24 38v6m-8-4v2m16-2v2"/></g>',
  fireworks: '<g fill="none"><path stroke="currentColor" stroke-linejoin="round" stroke-width="4" d="m6 42l8.674-24.736L31 34.038z"/><path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="4" d="m23 19l5-5q4-4 1-7m0 18l5-5q5-5 10 0"/><path fill="currentColor" d="M20 7a2 2 0 1 0 0-4a2 2 0 0 0 0 4m22-1a2 2 0 1 0 0-4a2 2 0 0 0 0 4m0 23a2 2 0 1 0 0-4a2 2 0 0 0 0 4m-3 9a2 2 0 1 0 0-4a2 2 0 0 0 0 4"/></g>',
  candy: '<g fill="none" stroke="currentColor" stroke-width="4"><circle cx="24" cy="24" r="10" stroke-linecap="round" stroke-linejoin="round"/><path stroke-linecap="round" d="M24 28a4 4 0 0 1-4-4"/><path stroke-linejoin="round" d="m16.688 16.813l-12.78-1.846L14.842 4.033zm14.625 14.5l12.779 1.845l-10.934 10.934z"/></g>',
  leaf: '<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="4" d="M37 24c0 14.094-13 20-13 20s-13-4.625-13-20S24 4 24 4s13 5.906 13 20M24 36l5-5m-5-2l-5-5m5-1l5-5m-5 26V14"/>',
  magic: '<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="4" d="m20.1 8.1l4.243 4.243M30 4v6zm9.9 4.1l-4.243 4.243zM44 18h-6zm-4.1 9.9l-4.243-4.243zM30 32v-6zm-9.9-4.1l4.243-4.243zM16 18h6zm13.586.414L5.544 42.456"/>'
}

export const MOOD_ICON_VIEWBOX = '0 0 48 48'
