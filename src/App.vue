<script setup lang="ts">
// 应用外壳：路由切换带过渡动画；全局消息出口交给 Element Plus（ElMessage）。
</script>

<template>
  <el-config-provider>
    <div class="app-shell">
      <router-view v-slot="{ Component }">
        <transition name="page" mode="out-in">
          <component :is="Component" :key="$route.path" />
        </transition>
      </router-view>
    </div>
  </el-config-provider>
</template>
