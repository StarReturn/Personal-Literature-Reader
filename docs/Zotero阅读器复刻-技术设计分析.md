# Zotero 阅读器复刻 · 技术设计分析

> 日期：2026-10-02 · 决策：**全部复刻**（本文档为开工前的完整技术分析）
> 原则：交互对齐 Zotero 7 Reader；架构渐进改造（不换 pdf.js 自带 viewer）；守住并整合我们独有的分栏对照、分析页码链接、AI 能力。

---

## 一、复刻目标全集（Zotero Reader 功能清单 → 我们的实现映射）

### A. 布局区（5 项）

| # | Zotero 行为 | 我们的实现 | 现状 |
| --- | --- | --- | --- |
| A1 | 左侧注释栏：按页列表、类型图标、颜色、引文摘录、备注、点击定位、hover 反查页面 | 常驻侧栏组件，数据源 pdf_annotations（已有），点击 scrollIntoView + 短暂闪烁高亮 | 有抽屉，需升级常驻+筛选+hover 联动 |
| A2 | 左侧缩略图栏：小图、当前页框、点击跳页 | scale 0.18 懒渲染队列（低优先级），双向联动 | ❌ |
| A3 | 左侧大纲栏：PDF 书签树，点击跳页 | `doc.getOutline()` + `getPageIndex(getDestination(...))`；无大纲退化为「分析栏目导航」（差异化） | ❌ |
| A4 | 侧栏三态切换 + 收起 | 工具栏左侧三 icon 切换 | ❌ |
| A5 | 注释/缩略图栏内注释标记（缩略图左上角彩点列） | 缩略图容器角落色点（每页注释聚合） | ❌ |

### B. 顶部工具栏（6 项）

| # | Zotero 行为 | 我们的实现 |
| --- | --- | --- |
| B1 | 页码 x/y 实时（滚动更新）+ 输入跳页 | 视口中心页计算（scroll 节流） |
| B2 | 缩放：− 百分比下拉 +（50%~400%、页宽适应、页高适应、实际大小） | zoom state + fitWidth/fitPage/actual 计算容器宽 |
| B3 | 布局切换：单页/双页/双页含封面 | 滚动容器 grid 1/2 列 + 封面 offset |
| B4 | 滚动模式：垂直/水平/翻页 | vertical（默认）/ horizontal（flex 横排）/ paged（复刻现有单页模式） |
| B5 | 旋转 90° | canvas transform + 占位宽高互换 |
| B6 | 打印 / 下载 | 系统打印（window.print 局部）/ 已有导出 |

### C. 阅读核心（6 项）

| # | Zotero 行为 | 我们的实现 |
| --- | --- | --- |
| C1 | **连续垂直滚动** | 多页容器 + IntersectionObserver 懒渲染（可视±2），离屏销毁 canvas 保占位高度 |
| C2 | 水平滚动 / 翻页模式 | C1 的方向变体；paged 即保留旧单页交互 |
| C3 | 平滑跳页（页码/链接/注释点击） | scrollIntoView({behavior:'smooth'}) + 中心定位 |
| C4 | ctrl+滚轮缩放 / 双指缩放 | wheel 事件（ctrlKey）± 步进，锚点保持视口中心 |
| C5 | 页面内超链接可点 | textLayer 的 link 注入（pdf.js getAnnotations → url 映射 overlay） |
| C6 | 阅读位置记忆升级 | 存 {mode, zoom, scrollOffset(页+页内比例)} 替代纯页码 |

### D. 注释体系（10 项，复刻重点）

| # | Zotero 行为 | 我们的实现 | 数据模型影响 |
| --- | --- | --- | --- |
| D1 | 选中文字 → 浮条：高亮四色/下划线/删除线/备注/复制 | selectionchange → getClientRects 定位浮条 | — |
| D2 | 高亮（文字矩形彩块） | 已有 highlight，升级为浮条触发 | 现有 |
| D3 | **下划线 / 删除线** | 文字 rects 的底边线/中线 SVG 元素 | kind + underline/strike（同 rects 结构） |
| D4 | 区域注释（框选图像区域，Zotero 截图进笔记） | 现有 rect 升级：框选时可勾选"捕获截图"（canvas 裁剪存 PNG，库 pets 同级目录 annot_images/） | 可选 image 列 |
| D5 | **墨迹手绘（ink）** | pointerdown/move 采集路径，SVG polyline | **需新列 strokes TEXT**（JSON 点集） |
| D6 | 文字便签（note，页面任意点） | 现有 note/便签体系 | 现有 |
| D7 | 注释备注（comment） | 已有 text | 现有 |
| D8 | **注释标签** | 注释级 tags（区别文献标签） | **需新列 tags TEXT** |
| D9 | 颜色自定义 + 最近使用 | 色板 + 自定义 hex + MRU | color 存 hex，兼容旧枚举 |
| D10 | 注释筛选（颜色/类型/标签） | 注释栏头部筛选条 | — |

### E. 检索与导航（3 项）

| # | Zotero 行为 | 我们的实现 |
| --- | --- | --- |
| E1 | Ctrl+F 全文搜索：结果计数、上下条、全部高亮、大小写 | 全文索引（智能导入已建提取逻辑复用）；命中项由 text item transform 计算矩形 → 临时高亮 overlay |
| E2 | 大纲导航（见 A3） | — |
| E3 | 注释列表 ↔ 页面双向定位 | A1 + scrollIntoView |

### F. 模式与杂项（5 项）

| # | Zotero 行为 | 我们的实现 |
| --- | --- | --- |
| F1 | 深色/浅色/自动 | CSS 滤镜（invert(1) hue-rotate(180deg) 作用于页面容器），注释色校准白名单 |
| F2 | 键盘快捷键 | j/k/PgUp/PgDn 翻页、+/− 缩放、Ctrl+F、g 跳页、Esc 关浮条 |
| F3 | 选中拖入笔记 | 拖拽：选中文字 → drag 到右侧笔记编辑器插入引用块（文献题录+页码）——**独有增强** |
| F4 | 注释导出 | 扩展现有导出：含注释段（类型/颜色/页码/摘录/备注） |
| F5 | 打印 | B6 |

---

## 二、数据模型迁移（SQLite）

```sql
ALTER TABLE pdf_annotations ADD COLUMN strokes TEXT;      -- ink 路径 JSON [[{x,y}..],..]，归一化坐标
ALTER TABLE pdf_annotations ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
ALTER TABLE pdf_annotations ADD COLUMN image_path TEXT;    -- 区域注释截图相对路径（可选）
```

- color 列兼容：旧枚举（yellow/red/blue/green）继续有效，新增任意 hex
- kind 集合扩展：highlight | underline | strike | rect | note | ink
- 迁移方式：启动时 pragma table_info 检测缺列则 ALTER（幂等）
- 备份/恢复自动兼容（整库备份）

## 三、前端架构重构设计

```
ReadView（保留：三布局切换/分栏/右侧面板）
 └ <PdfReader>（新组件，整体接管 PDF 面板；旧单页逻辑退役为 paged 模式）
    ├ <ReaderToolbar>   页码/缩放/布局/滚动模式/旋转/侧栏开关/搜索入口
    ├ <ReaderSidebar>   mode: annotations | thumbnails | outline（可收起）
    ├ <SearchPanel>     Ctrl+F 浮层
    └ <ScrollStage>     连续滚动容器（horizontal/paged 为变体）
        └ <PdfPageItem> ×N   占位 div（viewport 高）＋ 懒挂载：
             canvas(DPR) + textLayer + annoSvg（本页注释渲染）
             inkLayer（pointer 采集）+ 链接 overlay
```

**核心 composable：`usePageRenderer`**
- `pages: PageState[]`（index/rendered/rendering/height）
- IntersectionObserver(rootMargin 200%) 驱动 render/destroy
- zoom 全局变更 → 重排占位 + 重渲染可视页 + **锚点保持**（中心点内容不跳动）
- current page：scroll 节流计算中心页 → 工具栏/缩略图/位置记忆
- 缩略图渲染队列：独立低 scale，idle 优先级（requestIdleCallback）

**批注适配**：每页 annoSvg 渲染本页 annotations；新增元素类型：
- highlight：rect 彩块（现有）
- underline：每 text rect 底边 line
- strike：中线 line
- ink：polyline(strokes)
- rect/note：现有
选中浮条、常驻注释栏与 ScrollStage 通过 provide/inject 通信。

## 四、实施批次（全部复刻的执行顺序）

| 批次 | 内容 | 关键验收 |
| --- | --- | --- |
| **1 地基** | PdfReader 骨架 + 连续滚动渲染管理器（DPR/懒渲染/锚点缩放/ctrl+滚轮）+ 工具栏（B1/B2 页宽适应）+ 位置记忆升级 + paged 模式保留；旧批注暂只读展示 | 500 页流畅、内存稳；页码链接/分栏回归通过 |
| **2 注释复刻** | 选中浮条（D1）+ 下划线/删除线（D3）+ 墨迹（D5）+ 注释标签（D8）+ 自定义色/MRU（D9）+ 常驻注释栏与筛选（A1/D10）+ 数据迁移 | Zotero 六类注释齐；迁移幂等 |
| **3 导航检索** | 缩略图栏（A2/A5）+ 大纲栏（A3 含退化）+ Ctrl+F 全文搜索（E1）+ 链接可点（C5） | 双向定位正确 |
| **4 模式杂项** | 双页/滚动模式/旋转（B3/B4/B5）+ 深色模式（F1）+ 快捷键（F2）+ 拖选入笔记（F3）+ 注释进导出（F4） | 全快捷键可用 |
| **5 整合回归** | 与 AI 分析/智能导入/页码跳转/导出全链路回归 + 大 PDF 性能压测 + 视觉走查 | 验收清单全绿 |

## 五、风险与对策

| 风险 | 对策 |
| --- | --- |
| 多页化后批注坐标错位（DPR/占位/transform 叠加） | 坐标统一走"页内归一化"（现有体系不变），元素层每页独立原点 |
| 大 PDF 内存 | 离屏销毁 canvas、缩略图 idle 渲染、页高缓存 |
| textLayer 每页渲染开销 | 仅可视页渲染 textLayer（选择高亮只在可视页可用，符合直觉） |
| 旧批注数据兼容 | 颜色/类型枚举向后兼容；迁移幂等可回滚（备份先行） |
| 拖拽分隔与滚动容器事件冲突 | divider 独立 mousedown 层，容器 wheel 不拦截 |
