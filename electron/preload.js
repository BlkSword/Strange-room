const { contextBridge, ipcRenderer } = require('electron');

// 安全地暴露API到渲染进程
contextBridge.exposeInMainWorld('electronAPI', {
  // 应用信息
  getVersion: () => ipcRenderer.invoke('app-version'),

  // 对话框
  showMessageBox: (options) => ipcRenderer.invoke('show-message-box', options),
  showSaveDialog: (options) => ipcRenderer.invoke('show-save-dialog', options),

  // 打开外部链接
  openExternal: (url) => ipcRenderer.invoke('open-external', url),

  // 文件下载
  downloadFile: (url, filename) => ipcRenderer.invoke('download-file', url, filename),

  // 平台信息
  platform: process.platform,

  // 开发模式检测
  isDev: process.env.NODE_ENV !== 'production'
});

// 监听窗口事件
window.addEventListener('DOMContentLoaded', () => {
  // 通知主进程窗口已加载
  console.log('Electron preload loaded');
});
