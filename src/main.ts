import { createApp } from 'vue'
import { createPinia } from 'pinia'
import ElementPlus from 'element-plus'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import 'element-plus/dist/index.css'
import App from './App.vue'
import router from './router'
import './styles/ui-baseline.css'
import './assets/main.css'
import { initTheme } from './lib/theme'
import { api } from './ipc'

initTheme()

const app = createApp(App)
app.use(createPinia())
app.use(router)
app.use(ElementPlus, { locale: zhCn })
for (const [key, component] of Object.entries(ElementPlusIconsVue)) {
  app.component(key, component)
}

// 桌宠窗口（label=pet）加载同一个 SPA：挂载前引导到 /pet 路由，避免闪现主界面
async function boot() {
  if (api.isTauri && !location.hash) {
    try {
      const { getCurrent } = await import('@tauri-apps/api/window')
      if (getCurrent().label === 'pet') {
        location.hash = '#/pet'
      }
    } catch {
      /* 非 pet 窗口 */
    }
  }
  app.mount('#app')
}

void boot()
