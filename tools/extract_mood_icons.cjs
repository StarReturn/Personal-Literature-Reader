// 从 @iconify-json/icon-park-outline 提取主题情绪图标，生成零依赖的 moodIcons.ts
const icons = require('@iconify-json/icon-park-outline/icons.json')
const fs = require('fs')

const need = {
  sun: 'sun',
  moon: 'moon',
  rocket: 'rocket-one',
  fireworks: 'fireworks',
  candy: 'candy',
  leaf: 'leaf',
  magic: 'magic'
}

const out = []
for (const [key, name] of Object.entries(need)) {
  const i = icons.icons[name]
  if (!i) throw new Error('missing ' + name)
  out.push(`  ${key}: '${i.body}'`)
}

const ts = `// 情绪图标：取自 icon-park-outline（iconfont 风格，48x48 viewBox），
// 构建期从 @iconify-json/icon-park-outline 提取（值为完整 SVG 内部标记，自带描边属性），运行时零依赖。
export const MOOD_ICONS: Record<string, string> = {
${out.join(',\n')}
}

export const MOOD_ICON_VIEWBOX = '0 0 48 48'
`
fs.writeFileSync('src/lib/moodIcons.ts', ts)
console.log('regenerated src/lib/moodIcons.ts with', Object.keys(need).length, 'icons')
