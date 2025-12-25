const { app, BrowserWindow, ipcMain, dialog, shell } = require('electron');
const path = require('path');
const isDev = require('electron-is-dev');

let mainWindow;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 600,
    resizable: true,
    frame: true,
    title: 'Strange Room',
    backgroundColor: '#1a1a2e',
    show: false,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      enableRemoteModule: false,
      preload: path.join(__dirname, 'preload.js'),
      webSecurity: true
    }
  });

  // 窗口准备好后显示
  mainWindow.once('ready-to-show', () => {
    mainWindow.show();
  });

  // 加载应用
  if (isDev) {
    // 开发环境：加载 Next.js 开发服务器
    mainWindow.loadURL('http://localhost:3000');
    // 打开开发者工具
    mainWindow.webContents.openDevTools();
  } else {
    // 生产环境：需要先启动 Next.js 服务器，或使用 .next standalone 模式
    // 这里使用 http://localhost:3000 作为默认，需要确保服务已启动
    mainWindow.loadURL('http://localhost:3000');
  }

  // 拦截新窗口打开
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: 'deny' };
  });

  // 阻止导航到外部URL
  mainWindow.webContents.on('will-navigate', (event, url) => {
    const allowedOrigins = [
      'http://localhost:3000',
      'https://talk.blksword.com',
      'wss://talk.blksword.com'
    ];

    const urlObj = new URL(url);
    const isAllowed = allowedOrigins.some(origin => url.startsWith(origin));

    if (!isAllowed) {
      event.preventDefault();
      shell.openExternal(url);
    }
  });

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

// App事件
app.whenReady().then(() => {
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// IPC通信处理
ipcMain.handle('app-version', () => {
  return app.getVersion();
});

ipcMain.handle('show-message-box', async (event, options) => {
  const result = await dialog.showMessageBox(mainWindow, options);
  return result;
});

ipcMain.handle('show-save-dialog', async (event, options) => {
  const result = await dialog.showSaveDialog(mainWindow, options);
  return result;
});

ipcMain.handle('open-external', async (event, url) => {
  await shell.openExternal(url);
  return true;
});

// 处理下载
ipcMain.handle('download-file', async (event, url, filename) => {
  const { canceled, filePath } = await dialog.showSaveDialog(mainWindow, {
    defaultPath: filename,
    filters: [
      { name: 'All Files', extensions: ['*'] },
      { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] },
      { name: 'Videos', extensions: ['mp4', 'webm', 'mov'] }
    ]
  });

  if (canceled || !filePath) {
    return { canceled: true };
  }

  // 下载文件
  const net = require('net');
  const fs = require('fs');
  const https = require('https');

  return new Promise((resolve, reject) => {
    https.get(url, (response) => {
      if (response.statusCode === 302 || response.statusCode === 301) {
        // 处理重定向
        https.get(response.headers.location, (redirectResponse) => {
          const file = fs.createWriteStream(filePath);
          redirectResponse.pipe(file);
          file.on('finish', () => {
            file.close();
            resolve({ canceled: false, filePath });
          });
          file.on('error', (err) => {
            fs.unlink(filePath, () => {});
            reject(err);
          });
        });
      } else {
        const file = fs.createWriteStream(filePath);
        response.pipe(file);
        file.on('finish', () => {
          file.close();
          resolve({ canceled: false, filePath });
        });
        file.on('error', (err) => {
          fs.unlink(filePath, () => {});
          reject(err);
        });
      }
    }).on('error', (err) => {
      reject(err);
    });
  });
});

// 安全：CSP设置
app.on('web-contents-created', (event, contents) => {
  contents.on('will-attach-webview', (event, webPreferences, params) => {
    // 删除所有预加载脚本
    delete webPreferences.preload;
    delete webPreferences.preloadUrl;

    // 禁用节点集成
    webPreferences.nodeIntegration = false;
    webPreferences.contextIsolation = true;
  });

  contents.on('new-window', (event, url, frameName, disposition, options) => {
    if (url.startsWith('http:') || url.startsWith('https:')) {
      event.preventDefault();
      shell.openExternal(url);
    }
  });
});
