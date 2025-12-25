/**
 * Electron API 类型定义
 * 用于 preload.js 中暴露给渲染进程的 API
 */

export interface ElectronAPI {
  // 应用信息
  getVersion: () => Promise<string>;

  // 对话框
  showMessageBox: (options: MessageBoxOptions) => Promise<MessageBoxResult>;
  showSaveDialog: (options: SaveDialogOptions) => Promise<SaveDialogResult>;

  // 打开外部链接
  openExternal: (url: string) => Promise<boolean>;

  // 文件下载
  downloadFile: (url: string, filename: string) => Promise<DownloadResult>;

  // 平台信息
  platform: NodeJS.Platform;

  // 开发模式检测
  isDev: boolean;
}

export interface MessageBoxOptions {
  type?: 'none' | 'info' | 'warning' | 'error' | 'question';
  title?: string;
  message: string;
  detail?: string;
  buttons?: string[];
  defaultId?: number;
  cancelId?: number;
}

export interface MessageBoxResult {
  response: number;
  checkboxChecked?: boolean;
}

export interface SaveDialogOptions {
  title?: string;
  defaultPath?: string;
  buttonLabel?: string;
  filters?: FileFilter[];
  message?: string;
}

export interface FileFilter {
  name: string;
  extensions: string[];
}

export interface SaveDialogResult {
  canceled: boolean;
  filePath?: string;
}

export interface DownloadResult {
  canceled: boolean;
  filePath?: string;
}

declare global {
  interface Window {
    electronAPI: ElectronAPI;
  }
}

export {};
