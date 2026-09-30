<script setup lang="ts">
// Markdown 渲染组件：消毒后的 HTML + 页码链接转跳转交互。
import { computed, nextTick, ref, watch } from 'vue'
import { bindPageLinks, renderMarkdown } from '../lib/markdown'

const props = defineProps<{ md: string }>()
const emit = defineEmits<{ (e: 'goto-page', page: number): void }>()

const container = ref<HTMLElement | null>(null)
const html = computed(() => renderMarkdown(props.md))

watch(
  html,
  async () => {
    await nextTick()
    if (container.value) {
      bindPageLinks(container.value, (p) => emit('goto-page', p))
    }
  },
  { immediate: true }
)
</script>

<template>
  <div ref="container" class="md-body" v-html="html"></div>
</template>
