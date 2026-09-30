<script setup lang="ts">
// 桌宠窗口：透明置顶小窗中的角色形象 + 刚体动作引擎。
// 动作预设：待机呼吸 / 悬停 / 拖拽移动窗口 / 点击弹跳气泡 / 打盹 / 双击回主窗口。
// 形象：资料库 pets/ 中用户导入的图片（AI 生成），无图时使用应用默认兔子 logo。
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { api } from '../ipc'
import logoUrl from '../assets/logo.png'

// 默认形象：随应用内置的橘猫（public/ 静态资源）；加载失败回退应用 logo
const imageURL = ref<string>('/desktop-pet/ginger-tabby-portrait.png')
const bubble = ref<string | null>(null)
const bouncing = ref(false)
const sleeping = ref(false)

const LINES = [
  '今天读文献了吗？',
  '记得备份资料库哦～',
  '又有新论文要导入吗？',
  '对比表该更新啦',
  '喝口水休息一下吧',
  '笔记要常写才有效～',
  '我在这里陪你看文献'
]
const SLEEP_LINES = ['Zzz…', '（打盹中…轻轻点我）']

let bubbleTimer: ReturnType<typeof setTimeout> | null = null
let sleepTimer: ReturnType<typeof setTimeout> | null = null

function say(text: string, duration = 2600) {
  bubble.value = text
  if (bubbleTimer) clearTimeout(bubbleTimer)
  bubbleTimer = setTimeout(() => (bubble.value = null), duration)
}

function poke() {
  wake()
  bouncing.value = true
  setTimeout(() => (bouncing.value = false), 550)
  say(LINES[Math.floor(Math.random() * LINES.length)])
}

function wake() {
  if (sleeping.value) {
    sleeping.value = false
    say('（被叫醒了）')
  }
  scheduleSleep()
}

function scheduleSleep() {
  if (sleepTimer) clearTimeout(sleepTimer)
  sleepTimer = setTimeout(() => {
    sleeping.value = true
    say(SLEEP_LINES[0], 6000)
  }, 5 * 60 * 1000)
}

async function openMain() {
  const { Window } = await import('@tauri-apps/api/window')
  const win = Window.getByLabel('main')
  if (win) {
    await win.show()
    await win.unminimize()
    await win.setFocus()
  }
}

async function loadPetImage() {
  try {
    const bytes = await api.petGetImage()
    if (bytes) {
      const blob = new Blob([bytes as unknown as BlobPart], { type: 'image/png' })
      imageURL.value = URL.createObjectURL(blob)
    }
  } catch {
    // 读取失败使用默认形象
  }
}

function fallbackToLogo() {
  imageURL.value = logoUrl
}

function onActivity() {
  wake()
}

onMounted(async () => {
  await loadPetImage()
  scheduleSleep()
  setTimeout(() => say('我上线啦～双击回主界面', 3000), 800)
  window.addEventListener('mousemove', onActivity)
})

onBeforeUnmount(() => {
  if (bubbleTimer) clearTimeout(bubbleTimer)
  if (sleepTimer) clearTimeout(sleepTimer)
  window.removeEventListener('mousemove', onActivity)
})
</script>

<template>
  <div class="pet-stage">
    <div class="pet-bubble" :class="{ show: bubble }">{{ bubble || '' }}</div>
    <div
      class="pet-sprite"
      :class="{ bounce: bouncing, sleep: sleeping }"
      data-tauri-drag-region
      title="拖动移动位置；单击互动；双击回主界面"
      @click="poke"
      @dblclick="openMain"
    >
      <img :src="imageURL" alt="桌宠" draggable="false" @error="fallbackToLogo" />
    </div>
    <div v-if="sleeping" class="pet-zzz">💤</div>
  </div>
</template>

<style scoped>
.pet-stage {
  position: relative;
  width: 100vw;
  height: 100vh;
  background: transparent;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  overflow: visible;
  user-select: none;
}

.pet-sprite {
  width: 160px;
  height: 160px;
  margin-bottom: 30px;
  cursor: grab;
  animation: pet-float 3.2s ease-in-out infinite;
  transition: filter 0.3s;
  /* 允许整体拖拽窗口，同时保留子元素点击 */
  -webkit-app-region: no-drag;
}

.pet-sprite[data-tauri-drag-region] {
  -webkit-app-region: drag;
}

.pet-sprite:hover {
  animation-play-state: paused;
  transform: scale(1.08);
  filter: drop-shadow(0 6px 14px rgba(64, 158, 255, 0.35));
}

.pet-sprite:active {
  cursor: grabbing;
}

.pet-sprite img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  pointer-events: none;
  filter: drop-shadow(0 4px 10px rgba(0, 0, 0, 0.25));
}

.pet-sprite.sleep {
  animation: pet-sleep-breathe 5s ease-in-out infinite;
  filter: opacity(0.75);
}

/* 点击弹跳：squash & stretch */
.pet-sprite.bounce {
  animation: pet-bounce 0.55s cubic-bezier(0.28, 0.84, 0.42, 1);
}

/* 气泡 */
.pet-bubble {
  position: absolute;
  top: 8px;
  left: 50%;
  transform: translateX(-50%) translateY(-6px) scale(0.9);
  max-width: 190px;
  padding: 7px 12px;
  background: #fff;
  color: #333;
  border-radius: 12px;
  border-bottom-left-radius: 2px;
  border: 1px solid #e6e9ef;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.16);
  font-size: 12.5px;
  line-height: 1.5;
  white-space: nowrap;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.25s, transform 0.25s;
}

.pet-bubble.show {
  opacity: 1;
  transform: translateX(-50%) translateY(0) scale(1);
}

.pet-zzz {
  position: absolute;
  right: 18px;
  bottom: 120px;
  font-size: 20px;
  animation: pet-zzz-float 2.6s ease-in-out infinite;
  pointer-events: none;
}

@keyframes pet-float {
  0%,
  100% {
    transform: translateY(0) rotate(-1.2deg);
  }
  50% {
    transform: translateY(-9px) rotate(1.2deg);
  }
}

@keyframes pet-sleep-breathe {
  0%,
  100% {
    transform: translateY(0) scale(1, 1);
  }
  50% {
    transform: translateY(-3px) scale(1.02, 0.98);
  }
}

@keyframes pet-bounce {
  0% {
    transform: scale(1, 1) translateY(0);
  }
  25% {
    transform: scale(1.15, 0.8) translateY(8px);
  }
  55% {
    transform: scale(0.9, 1.15) translateY(-16px);
  }
  80% {
    transform: scale(1.05, 0.92) translateY(2px);
  }
  100% {
    transform: scale(1, 1) translateY(0);
  }
}

@keyframes pet-zzz-float {
  0%,
  100% {
    transform: translateY(0);
    opacity: 0.9;
  }
  50% {
    transform: translateY(-8px);
    opacity: 0.5;
  }
}
</style>
