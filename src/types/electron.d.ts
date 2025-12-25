/**
 * 全局类型定义 - Electron API
 */

declare global {
  interface Window {
    electronAPI: {
      // 应用信息
      getVersion: () => Promise<string>;

      // 对话框
      showMessageBox: (options: {
        type?: 'none' | 'info' | 'warning' | 'error' | 'question';
        title?: string;
        message: string;
        detail?: string;
        buttons?: string[];
      }) => Promise<{ response: number }>;

      showSaveDialog: (options: {
        title?: string;
        defaultPath?: string;
        filters?: Array<{
          name: string;
          extensions: string[];
        }>;
      }) => Promise<{
        canceled: boolean;
        filePath?: string;
      }>;

      // 打开外部链接
      openExternal: (url: string) => Promise<boolean>;

      // 文件下载
      downloadFile: (url: string, filename: string) => Promise<{
        canceled: boolean;
        filePath?: string;
      }>;

      // 平台信息
      platform: NodeJS.Platform;

      // 开发模式检测
      isDev: boolean;
    };
  }
}

export {};
