# Litwright · 文献综述中心

> Litwright = literature + wright（匠）：文献匠——个人本地文献阅读与对比工作台。

个人本地文献阅读与对比工作台。外部 AI 按固定模板生成分析 Markdown，本应用负责 PDF+MD 配对导入、SQLite 归档、PDF 分栏阅读、2～5 篇结构化对比、个人笔记与综合结论，全部数据存放本机。

> 需求依据：`文献综合整理网站-需求文档.md`（v0.2）

## 技术栈

- **前端**：Vue 3 + TypeScript + Vite；pdf.js（PDF 渲染）、marked + DOMPurify + KaTeX（Markdown 与公式），全部本地打包，断网可用。
- **核心**：Rust 库 `litreview-core`（rusqlite/SQLite、MD 解析器、导入/对比/备份逻辑），业务逻辑只写一份。
- **两个外壳共用同一核心**：
  - `litreview-dev-server`（axum）：浏览器模式，开发与自动化测试用；构建后同端口直接服务前端，即"本地网页应用"。
  - `litreview-app`（Tauri 2）：桌面模式，双击窗口，后续打包 Windows EXE（P1 目标）。
- 前端通过 `src/ipc` 适配器在 IPC / HTTP 之间自动切换，页面代码不感知运行环境。

## 目录结构

```text
├── crates/core            # Rust 核心：db / mdparse / service（papers·import·compares·export·backup）
├── crates/dev-server      # axum HTTP 镜像（127.0.0.1:8787）
├── src-tauri              # Tauri 2 桌面外壳（命令层）
├── src/                   # Vue 前端（views / components / ipc / lib / stores）
├── 文献AI分析模板.md       # 交给外部 AI 的输出模板（应用内可下载）
├── samples/               # 验收用样例 PDF + MD
├── tools/make_sample_pdf.py
└── data/                  # 运行时数据（资料库/配置/临时导入），可整体迁移
    └── LiteratureLibrary/
        ├── library.sqlite
        └── papers/<paper-id>/original.pdf
```

## 启动

### 浏览器模式（开发 / 日常使用）

```bash
npm install
npm run dev:server     # 终端 1：核心 + HTTP API（127.0.0.1:8787）
npm run dev            # 终端 2：Vite 开发服务器（http://localhost:5173，/api 自动代理）
```

构建静态版后 `npm run dev:server` 会直接在 8787 端口提供完整应用：

```bash
npm run build && npm run dev:server   # 打开 http://127.0.0.1:8787
```

### 桌面模式（Tauri 窗口 / Windows EXE）

```bash
npm run tauri dev      # 开发窗口
npm run tauri build    # 产出 Windows 安装包（NSIS）+ 便携 EXE
```

打包产物：

- 安装包：`src-tauri/target/release/bundle/nsis/文献综述中心_0.1.0_x64-setup.exe`
- 便携版：`src-tauri/target/release/litreview-app.exe`（单文件直接双击运行，无需安装）

桌面版数据目录固定在 `%APPDATA%\com.personal.litreview\`（资料库独立于安装目录，升级与卸载不清数据）；首次运行会自动创建。窗口标题与图标使用根目录 `app-icon.png` 生成的图标集。

## 使用流程

1. 在应用外用 AI（连同 `文献AI分析模板.md` 与需求文档第 5 节规则）生成分析 MD。
2. 「导入文献」打开导入对话框：单篇模式（步骤式：选择文件 → 解析预览 → 修正元数据入库）或批量模式（一次多选 PDF 与 MD，同名自动配对，配对表预览后一键全部导入，支持统一归入项目）；支持仅 PDF、仅粘贴 MD、更新已有文献（保留恢复副本与笔记）。
3. 文献库：搜索（标题/作者/DOI/标签/分析/笔记，并提示命中来源）、项目/标签/状态筛选、回收站（移入不删除，永久删除仅删托管副本）。
4. 阅读页：PDF 翻页/跳转/缩放 + 分栏（可拖拽分隔），阅读位置自动记忆；PDF 批注——选中文字松开即按当前颜色生成高亮，或用框选工具拖拽标注，四色语义（黄重点/红质疑/蓝同意/绿待查），批注可附备注、点击编辑/删除，列表点击跳页，坐标归一化与缩放无关，PDF 原文件永不修改；分析中的 `[PDF 第 N 页](#pdf-page-N)` 点击跳转；页码链接可标记"已核查"，内容变更后自动降级为待核查；个人笔记独立保存；可直接编辑分析 MD。
5. 对比页：2～5 篇 × 固定维度表格，维度可勾选、文献可排序增删；点击页码在抽屉中预览原文；分析更新后打开对比会明确提示哪些文献已更新；综合结论（共识/分歧/研究空白/个人结论）随对比保存。
6. 导出：单篇（分析+笔记）、多篇合并（勾选后"导出所选"或头部"导出全部"，含目录）、对比综述 Markdown；内部页码链接转为纯文本页码，便于外部追溯。
7. 备份：完整 ZIP（SQLite 快照 + 托管 PDF），恢复前校验完整性并可恢复到新资料库目录。

## 数据与安全

- 资料库默认在 `data/LiteratureLibrary/`，可在设置页切换；独立于应用目录，升级不清数据。
- Markdown 渲染经 DOMPurify 消毒：不执行脚本、不加载远程图片。
- 分析重新导入/编辑时保留更新前副本（可一键恢复）；个人笔记永不被分析覆盖。

## 已知边界（对应需求文档）

- 扫描版 PDF 可阅读，OCR 与 PDF 全文提取属于 P1+。
- 对比页导出是静态快照；应用内页码链接不承诺在外部阅读器打开 PDF。
- MD 附带本地图片资源包暂不支持（模板默认纯文本/表格/公式）。

## 验收

`验收标准`对照需求文档第 10 节 11 条（samples/ 提供测试数据）：

```bash
cargo test -p litreview-core   # MD 解析器与服务层单元测试
```

端到端（浏览器模式）：导入 samples 三篇 → 搜索/阅读/核查/笔记 → 三篇对比并保存导出 → 重启恢复 → 备份恢复到新目录。
