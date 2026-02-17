/**
 * Monaco Editor 本地化配置
 * 使用本地文件而不是 CDN
 */

export const monacoConfig = {
  // 使用本地 Monaco Editor 文件
  paths: {
    vs: '/monaco-editor/min/vs'
  }
};

/**
 * 初始化 Monaco Editor 配置
 * 必须在编辑器挂载前调用
 */
export function initMonacoConfig() {
  if (typeof window !== 'undefined') {
    // @ts-expect-error - Monaco 全局配置
    window.MonacoEnvironment = {
      getWorkerUrl: function (workerId: string, _label: string) {
        return `/monaco-editor/min/vs/base/workers/${workerId}`;
      }
    };
  }
}
