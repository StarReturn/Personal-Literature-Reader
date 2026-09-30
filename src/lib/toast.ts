// 统一消息出口：委托给 Element Plus 的 ElMessage（自带动画与主题）。
import { ElMessage, ElMessageBox } from 'element-plus'

export function toastOk(text: string): void {
  ElMessage.success(text)
}

export function toastError(text: string): void {
  ElMessage.error(text)
}

export function toastInfo(text: string): void {
  ElMessage.info(text)
}

/** 危险操作二次确认（替代 window.confirm，带动画与可访问性）。 */
export async function confirmAction(message: string, title = '请确认', confirmText = '确定'): Promise<boolean> {
  try {
    await ElMessageBox.confirm(message, title, {
      confirmButtonText: confirmText,
      cancelButtonText: '取消',
      type: 'warning',
      autofocus: false
    })
    return true
  } catch {
    return false
  }
}

/** 单行文本输入弹窗（用于新建/重命名项目等）。 */
export async function promptText(title: string, placeholder = '', initial = ''): Promise<string | null> {
  try {
    const r = await ElMessageBox.prompt(placeholder, title, {
      confirmButtonText: '确定',
      cancelButtonText: '取消',
      inputValue: initial,
      inputValidator: (v: string) => (v && v.trim() ? true : '内容不能为空')
    })
    return (r.value as string).trim()
  } catch {
    return null
  }
}
