import { createRouter, createWebHashHistory } from 'vue-router'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'library', component: () => import('../views/LibraryView.vue') },
    { path: '/read/:id', name: 'read', component: () => import('../views/ReadView.vue') },
    { path: '/pet', name: 'pet', component: () => import('../views/PetView.vue') },
    { path: '/compare/new', name: 'compare-new', component: () => import('../views/CompareView.vue') },
    { path: '/compare/:id', name: 'compare', component: () => import('../views/CompareView.vue') }
  ]
})

export default router
