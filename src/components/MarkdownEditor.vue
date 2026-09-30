<script setup lang="ts">
// Markdown 编辑器：基于 bytemd（字节开源）双栏所见即所得。
// 内核保存的仍是纯 Markdown 文本，与应用的解析/对比/导出链路完全兼容。
import { Editor } from '@bytemd/vue-next'
import gfm from '@bytemd/plugin-gfm'
import math from '@bytemd/plugin-math'
import zhHans from 'bytemd/locales/zh_Hans.json'
import 'bytemd/dist/index.css'

const props = defineProps<{
  modelValue: string
  placeholder?: string
  /** split = 双栏（默认）；tab = 纯编辑+预览切换 */
  mode?: 'split' | 'tab'
}>()
const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>()

const plugins = [gfm(), math()]

function handleChange(v: string) {
  emit('update:modelValue', v)
}
</script>

<template>
  <div class="md-editor-wrap" :class="{ 'mode-tab': mode === 'tab' }">
    <Editor
      :value="modelValue"
      :plugins="plugins"
      :locale="zhHans"
      :mode="mode || 'split'"
      :placeholder="placeholder"
      :preview-debounce="200"
      @change="handleChange"
    />
  </div>
</template>

<style scoped>
.md-editor-wrap {
  flex: 1;
  min-height: 320px;
  display: flex;
  flex-direction: column;
  border: 1px solid #e2e5ea;
  border-radius: 6px;
  overflow: hidden;
  background: #fff;
}

.md-editor-wrap :deep(.bytemd) {
  flex: 1;
  height: 100%;
  font-family: 'Segoe UI', 'Microsoft YaHei', system-ui, sans-serif;
}

/* 主题对齐 Element Plus 观感 */
.md-editor-wrap :deep(.bytemd-toolbar) {
  background: #f7f8fa;
  border-bottom: 1px solid #e6e9ef;
}

.md-editor-wrap :deep(.bytemd-toolbar-icon:hover) {
  background: var(--el-color-primary-light-9);
  color: var(--el-color-primary);
}

.md-editor-wrap :deep(.bytemd-toolbar-icon.bytemd-tippy-right:last-child:hover),
.md-editor-wrap :deep(.bytemd-toolbar-tab span:hover) {
  color: var(--el-color-primary);
}

.md-editor-wrap :deep(.bytemd-editor .CodeMirror) {
  font-family: Consolas, 'Microsoft YaHei', monospace;
  font-size: 13px;
  line-height: 1.6;
  height: 100%;
}

/* 预览区与站内 Markdown 展示风格一致 */
.md-editor-wrap :deep(.bytemd-preview) {
  font-size: 14px;
  line-height: 1.7;
  padding: 12px 16px;
}

.md-editor-wrap :deep(.bytemd-preview table) {
  border-collapse: collapse;
}

.md-editor-wrap :deep(.bytemd-preview th),
.md-editor-wrap :deep(.bytemd-preview td) {
  border: 1px solid #e2e5ea;
  padding: 5px 10px;
  font-size: 13px;
}

.md-editor-wrap :deep(.bytemd-preview th) {
  background: #f6f8fa;
}

.md-editor-wrap :deep(.bytemd-preview blockquote) {
  margin: 0.6em 0;
  padding: 4px 12px;
  border-left: 3px solid var(--el-color-primary);
  background: #f7f9fe;
  color: #6a737d;
}

.md-editor-wrap :deep(.bytemd-preview code) {
  background: #f6f8fa;
  border-radius: 4px;
  padding: 1px 5px;
  font-size: 0.92em;
}

.md-editor-wrap :deep(.bytemd-preview pre) {
  background: #f6f8fa;
  border-radius: 6px;
  padding: 10px 12px;
}

/* KaTeX 长公式滚动 */
.md-editor-wrap :deep(.katex-display) {
  overflow-x: auto;
  overflow-y: hidden;
}

/* 保险：隐藏 KaTeX 的 MathML 辅助副本（屏幕阅读器用），防止 CSS 未生效时公式重复显示 */
.md-editor-wrap :deep(.katex-mathml) {
  position: absolute;
  clip: rect(1px, 1px, 1px, 1px);
  padding: 0;
  border: 0;
  height: 1px;
  width: 1px;
  overflow: hidden;
}
</style>
