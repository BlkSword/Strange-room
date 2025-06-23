42
const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');

// 创建 WebSocket 服务器
const wss = new WebSocket.Server({ port: 8080 });

// 保存所有连接的客户端及其用户名
const clients = new Map();

wss.on('connection', (ws) => {
    console.log('Client connected.');

  // 临时存储新连接用户的信息
    let username = null;

  // 接收消息
    ws.on('message', (message) => {
    const data = message.toString();

    // 第一次连接，设置用户名
    if (!username) {
        username = data.trim();
        clients.set(ws, username);
        broadcastUserList();
        broadcastMessage(`[系统消息] ${username} 加入了聊天室`);
        return;
    }

    // 正常消息
    if (data) {
        broadcastMessage(`${username}: ${data}`);
    }
    });

  // 客户端断开连接
    ws.on('close', () => {
    console.log('Client disconnected.');
    if (username) {
        broadcastMessage(`[系统消息] ${username} 离开了聊天室`);
        clients.delete(ws);
        broadcastUserList();
    }
});

  // 错误处理
  ws.on('error', (err) => {
    console.error('WebSocket error:', err);
  });
});

// 广播消息给所有客户端
function broadcastMessage(message) {
  const msg = JSON.stringify({ type: 'message', content: message });
  wss.clients.forEach((client) => {
    if (client.readyState === WebSocket.OPEN) {
      client.send(msg);
    }
  });
}

// 广播当前在线用户列表
function broadcastUserList() {
  const users = Array.from(clients.values());
  const userListMsg = JSON.stringify({ type: 'users', list: users });
  wss.clients.forEach((client) => {
    if (client.readyState === WebSocket.OPEN) {
      client.send(userListMsg);
    }
  });
}

console.log('WebSocket 聊天服务器运行在 ws://localhost:8080');