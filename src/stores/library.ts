// 跨页面共享的分类数据：项目、标签、已保存对比。
import { defineStore } from 'pinia'
import { api, type CompareRecord, type ProjectInfo, type TagInfo } from '../ipc'

export const useLibraryStore = defineStore('library', {
  state: () => ({
    projects: [] as ProjectInfo[],
    tags: [] as TagInfo[],
    compares: [] as CompareRecord[],
    loaded: false
  }),
  actions: {
    async refresh() {
      const [projects, tags, compares] = await Promise.all([
        api.listProjects(),
        api.listTags(),
        api.listCompares()
      ])
      this.projects = projects
      this.tags = tags
      this.compares = compares
      this.loaded = true
    },
    async ensureLoaded() {
      if (!this.loaded) await this.refresh()
    }
  }
})
