/**
 * Strange Room 信令服务器
 * 功能：
 * 1. 房间管理 - 创建房间、检查房间是否存在
 * 2. 令牌管理 - 生成自包含令牌（包含房间ID和过期时间）
 * 3. WebRTC 信令 - 转发 WebRTC 信令消息
 */

const WebSocket = require('ws');
const http = require('http');
const { URL } = require('url');

// ========== 配置 ==========
const PORT = process.env.PORT || 9001;
const TOKEN_EXPIRY = 30 * 24 * 60 * 60 * 1000; // 30天
const ROOM_EXPIRY = 48 * 60 * 60 * 1000; // 48小时

// ========== 数据存储 ==========
const rooms = new Map(); // roomId -> { createdAt, expiresAt, creator }
const roomClients = new Map(); // roomId -> Set<WebSocket>
const rateLimits = new Map(); // IP -> { count, resetTime }

// ========== 安全配置 ==========
const MAX_REQUESTS_PER_MINUTE = 60;
const MAX_CONNECTIONS_PER_IP = 5;
// 允许所有来源
const ALLOWED_ORIGINS = ['*'];

// 速率限制检查
function checkRateLimit(ip) {
  const now = Date.now();
  const limit = rateLimits.get(ip);

  if (!limit || now > limit.resetTime) {
    rateLimits.set(ip, { count: 1, resetTime: now + 60000 });
    return true;
  }

  if (limit.count >= MAX_REQUESTS_PER_MINUTE) {
    return false;
  }

  limit.count++;
  return true;
}

// 获取客户端 IP
function getClientIP(req) {
  return req.headers['x-forwarded-for']?.split(',')[0] ||
         req.headers['x-real-ip'] ||
         req.socket.remoteAddress ||
         'unknown';
}

// 检查 CORS origin
function checkOrigin(origin) {
  // 允许所有来源
  return true;
}

// 安全日志
function securityLog(event, details) {
  const timestamp = new Date().toISOString();
  console.log(`[Security] ${timestamp} | ${event} | ${JSON.stringify(details)}`);
}

// ========== 辅助函数 ==========

// 生成加密安全的随机字符串
function generateSecureRandom(length) {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  let result = '';
  const randomValues = require('crypto').randomBytes(length);
  for (let i = 0; i < length; i++) {
    result += chars[randomValues[i] % chars.length];
  }
  return result;
}

// 生成随机房间ID（8位，使用加密安全随机数）
function generateRoomId() {
  let id;
  do {
    id = generateSecureRandom(8);
  } while (rooms.has(id));
  return id;
}

// 生成 HMAC 签名
function generateSignature(payload) {
  const secret = process.env.TOKEN_SECRET || 'strange-room-secret-change-in-production';
  const hmac = require('crypto').createHmac('sha256', secret);
  hmac.update(payload);
  return hmac.digest('hex');
}

// 验证签名
function verifySignature(payload, signature) {
  const expectedSignature = generateSignature(payload);
  // 使用 timing-safe 比较
  return require('crypto').timingSafeEqual(
    Buffer.from(signature),
    Buffer.from(expectedSignature)
  );
}

// 生成访问令牌（带签名）
function generateToken(roomId) {
  const expiresAt = Date.now() + TOKEN_EXPIRY;
  const nonce = generateSecureRandom(16); // 随机数防止重放
  const payload = `${roomId}|${expiresAt}|${nonce}`;

  // 添加签名
  const signature = generateSignature(payload);
  const tokenPayload = `${payload}|${signature}`;

  return Buffer.from(tokenPayload).toString('base64');
}

// 验证并解码令牌
function validateToken(token) {
  if (!token) return { valid: false, error: 'Missing token' };

  try {
    // Base64 解码
    const tokenPayload = Buffer.from(token, 'base64').toString('utf-8');
    const parts = tokenPayload.split('|');

    if (parts.length !== 4) {
      return { valid: false, error: 'Invalid token format' };
    }

    const [roomId, expiresAt, nonce, signature] = parts;
    const expiryTime = parseInt(expiresAt, 10);

    // 检查是否过期
    if (Date.now() > expiryTime) {
      return { valid: false, error: 'Token expired' };
    }

    // 验证签名
    const payload = `${roomId}|${expiresAt}|${nonce}`;
    if (!verifySignature(payload, signature)) {
      return { valid: false, error: 'Invalid signature' };
    }

    return { valid: true, roomId };
  } catch (error) {
    return { valid: false, error: 'Invalid token' };
  }
}

// 清理过期房间
function cleanupExpiredRooms() {
  const now = Date.now();
  for (const [roomId, room] of rooms.entries()) {
    if (now > room.expiresAt) {
      console.log(`[Server] 清理过期房间: ${roomId}`);
      rooms.delete(roomId);
      roomClients.delete(roomId);
    }
  }
}

// 定期清理（每分钟）
setInterval(() => {
  cleanupExpiredRooms();
}, 60 * 1000);

// ========== HTTP 服务器（用于 API 请求） ==========

const server = http.createServer((req, res) => {
  const clientIP = getClientIP(req);
  const origin = req.headers.origin;

  // CORS - 允许所有来源
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

  // 安全头
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Frame-Options', 'DENY');
  res.setHeader('X-XSS-Protection', '1; mode=block');

  if (req.method === 'OPTIONS') {
    res.writeHead(200);
    res.end();
    return;
  }

  // 速率限制
  if (!checkRateLimit(clientIP)) {
    securityLog('RATE_LIMIT_EXCEEDED', { ip: clientIP, path: pathname });
    res.writeHead(429, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: 'Too many requests' }));
    return;
  }

  const url = new URL(req.url, `http://${req.headers.host}`);
  const pathname = url.pathname;

  // API: 创建房间
  if (pathname === '/api/room/create' && req.method === 'POST') {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      try {
        const { ttl, creatorName } = JSON.parse(body);
        const roomId = generateRoomId();
        const now = Date.now();
        const expiresAt = now + (ttl * 60 * 60 * 1000);

        // 保存房间
        rooms.set(roomId, {
          createdAt: now,
          expiresAt,
          creator: creatorName || '未知',
        });

        // 初始化房间客户端集合
        roomClients.set(roomId, new Set());

        console.log(`[Server] 创建房间: ${roomId}, 过期时间: ${new Date(expiresAt).toISOString()}`);

        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          roomId,
          expiresAt,
        }));
      } catch (error) {
        console.error('[Server] 创建房间错误:', error);
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ success: false, error: 'Invalid request' }));
      }
    });
    return;
  }

  // API: 检查房间是否存在
  if (pathname.startsWith('/api/room/check/') && req.method === 'GET') {
    const roomId = pathname.split('/').pop();
    const room = rooms.get(roomId);

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      exists: !!room,
      room: room ? {
        createdAt: room.createdAt,
        expiresAt: room.expiresAt,
        creator: room.creator,
      } : null,
    }));
    return;
  }

  // API: 生成访问令牌（新接口，返回自包含令牌）
  if (pathname === '/api/token/generate' && req.method === 'POST') {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      try {
        const { roomId } = JSON.parse(body);

        // 验证房间是否存在
        if (!rooms.has(roomId)) {
          res.writeHead(404, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ success: false, error: 'Room not found' }));
          return;
        }

        // 生成自包含令牌
        const token = generateToken(roomId);

        console.log(`[Server] 生成令牌: ${token.substring(0, 16)}... 用于房间: ${roomId}`);

        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          success: true,
          token,
          expiresAt: Date.now() + TOKEN_EXPIRY,
        }));
      } catch (error) {
        console.error('[Server] 生成令牌错误:', error);
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ success: false, error: 'Invalid request' }));
      }
    });
    return;
  }

  // API: 验证令牌
  if (pathname === '/api/token/validate' && req.method === 'POST') {
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', () => {
      try {
        const { token } = JSON.parse(body);
        const result = validateToken(token);

        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(result));
      } catch (error) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ success: false, error: 'Invalid request' }));
      }
    });
    return;
  }

  // 健康检查
  if (pathname === '/health' || pathname === '/') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      name: 'Strange Room Signaling Server',
      version: '3.0.0',
      status: 'running',
      rooms: rooms.size,
      connections: Array.from(roomClients.values()).reduce((sum, set) => sum + set.size, 0),
    }));
    return;
  }

  // 404
  res.writeHead(404);
  res.end('Not Found');
});

// ========== WebSocket 服务器（用于 WebRTC 信令） ==========

const wss = new WebSocket.Server({ noServer: true });

// ========== WebSocket 服务器（用于 y-websocket 协议） ==========

// y-websocket 存储结构
const yjsRooms = new Map(); // roomId -> Set<WebSocket>
const yjsClients = new Map(); // ws -> { roomId, token }

function getYjsRoomClients(roomId) {
  if (!yjsRooms.has(roomId)) {
    yjsRooms.set(roomId, new Set());
  }
  return yjsRooms.get(roomId);
}

const yjsWss = new WebSocket.Server({ noServer: true });

// 处理 HTTP 升级请求，区分 y-websocket 和其他 WebSocket
server.on('upgrade', (req, socket, head) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const pathname = url.pathname;

  console.log(`[Server] Upgrade request: pathname=${pathname}`);

  // y-websocket 使用 /yjs/roomId 格式
  if (pathname.startsWith('/yjs')) {
    console.log(`[Server] Routing to yjs WebSocket server`);
    yjsWss.handleUpgrade(req, socket, head, (ws) => {
      yjsWss.emit('connection', ws, req);
    });
  } else if (pathname === '/signaling') {
    console.log(`[Server] Routing to signaling WebSocket server`);
    wss.handleUpgrade(req, socket, head, (ws) => {
      wss.emit('connection', ws, req);
    });
  } else {
    console.log(`[Server] Unknown WebSocket path: ${pathname}`);
    socket.destroy();
  }
});

yjsWss.on('connection', (ws, req) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  // y-websocket 会在路径后添加 /roomId，例如 /yjs/GGRFBNYN
  const pathname = url.pathname;
  const roomId = pathname.startsWith('/yjs/') ? pathname.split('/').pop() : url.searchParams.get('room');
  const token = url.searchParams.get('token');
  const clientIP = getClientIP(req);

  console.log(`[Yjs-WS] 连接请求: pathname=${pathname}, roomId=${roomId}, ip=${clientIP}`);

  // 验证房间
  if (!roomId) {
    securityLog('YJS_WS_REJECTED', { reason: 'missing_roomId', ip: clientIP });
    console.log('[Yjs-WS] 拒绝连接: 缺少房间ID');
    ws.close(1008, 'Missing roomId');
    return;
  }

  // 检查房间是否存在
  const room = rooms.get(roomId);
  if (!room) {
    securityLog('YJS_WS_REJECTED', { reason: 'room_not_found', roomId, ip: clientIP });
    console.log(`[Yjs-WS] 拒绝连接: 房间不存在 ${roomId}`);
    ws.close(1008, 'room_not_found');
    return;
  }

  // 验证令牌
  const tokenValidation = validateToken(token);
  if (!tokenValidation.valid || tokenValidation.roomId !== roomId) {
    securityLog('YJS_WS_REJECTED', {
      reason: 'invalid_token',
      roomId,
      error: tokenValidation.error,
      ip: clientIP
    });
    console.log(`[Yjs-WS] 拒绝连接: 无效令牌 - ${tokenValidation.error || 'Room mismatch'}`);
    ws.close(1008, 'Invalid token');
    return;
  }

  // 添加到房间
  const clients = getYjsRoomClients(roomId);
  clients.add(ws);
  yjsClients.set(ws, { roomId, token });

  console.log(`[Yjs-WS] 连接成功: roomId=${roomId}, 当前连接数: ${clients.size}`);

  // 监听消息（y-websocket 协议 - 使用二进制消息）
  ws.on('message', (data) => {
    const clientInfo = yjsClients.get(ws);

    if (!clientInfo) {
      ws.close(1008, 'Client not registered');
      return;
    }

    // y-websocket 使用二进制协议，直接转发给其他客户端
    const clients = getYjsRoomClients(clientInfo.roomId);
    let forwardedCount = 0;

    clients.forEach((client) => {
      if (client !== ws && client.readyState === WebSocket.OPEN) {
        client.send(data);  // 转发原始二进制数据
        forwardedCount++;
      }
    });

    if (forwardedCount > 0) {
      console.log(`[Yjs-WS] 转发消息给 ${forwardedCount} 个客户端，消息长度: ${data.length}`);
    }
  });

  // 连接关闭
  ws.on('close', () => {
    const clientInfo = yjsClients.get(ws);
    if (clientInfo) {
      const clients = getYjsRoomClients(clientInfo.roomId);
      clients.delete(ws);
      yjsClients.delete(ws);
      console.log(`[Yjs-WS] 连接关闭: roomId=${clientInfo.roomId}, 剩余连接数: ${clients.size}`);
    }
  });

  ws.on('error', (error) => {
    console.error(`[Yjs-WS] WebSocket 错误:`, error.message);
  });
});

wss.on('connection', (ws, req) => {
  const url = new URL(req.url, `ws://${req.headers.host}`);
  const roomId = url.searchParams.get('roomId');
  const token = url.searchParams.get('token');
  const clientIP = getClientIP(req);

  console.log(`[Server] WebSocket 连接请求: roomId=${roomId}, ip=${clientIP}`);

  // 验证房间
  if (!roomId) {
    securityLog('WS_REJECTED', { reason: 'missing_roomId', ip: clientIP });
    console.log('[Server] 拒绝连接: 缺少房间ID');
    ws.close(1008, 'Missing roomId');
    return;
  }

  // 检查房间是否存在
  const room = rooms.get(roomId);
  if (!room) {
    securityLog('WS_REJECTED', { reason: 'room_not_found', roomId, ip: clientIP });
    console.log(`[Server] 拒绝连接: 房间不存在 ${roomId}`);
    ws.close(1008, 'Room not found');
    return;
  }

  // 检查房间是否过期
  if (Date.now() > room.expiresAt) {
    securityLog('WS_REJECTED', { reason: 'room_expired', roomId, ip: clientIP });
    console.log(`[Server] 拒绝连接: 房间已过期 ${roomId}`);
    ws.close(1008, 'Room expired');
    return;
  }

  // 验证令牌
  const tokenValidation = validateToken(token);
  if (!tokenValidation.valid || tokenValidation.roomId !== roomId) {
    securityLog('WS_REJECTED', {
      reason: 'invalid_token',
      roomId,
      tokenPrefix: token?.substring(0, 16),
      error: tokenValidation.error,
      ip: clientIP
    });
    console.log(`[Server] 拒绝连接: 无效令牌 - ${tokenValidation.error || 'Room mismatch'}`);
    ws.close(1008, 'Invalid token');
    return;
  }

  // 添加到房间客户端集合
  if (!roomClients.has(roomId)) {
    roomClients.set(roomId, new Set());
  }
  roomClients.get(roomId).add(ws);

  securityLog('WS_CONNECTED', { roomId, ip: clientIP, connections: roomClients.get(roomId).size });
  console.log(`[Server] 连接成功: roomId=${roomId}, 当前连接数: ${roomClients.get(roomId).size}`);

  // 监听消息（y-webrtc 协议：简单转发）
  ws.on('message', (data) => {
    // 调试日志：记录收到的消息
    try {
      const message = JSON.parse(data.toString());
      console.log(`[Server] 收到 JSON 消息: roomId=${roomId}, type=${message.type || 'unknown'}`);
      console.log(`[Server] 消息内容:`, JSON.stringify(message).substring(0, 200));
    } catch {
      // 二进制消息（y-webrtc 使用二进制协议）
      console.log(`[Server] 收到二进制消息: roomId=${roomId}, 长度=${data.length}, 前10字节=${Array.from(data.slice(0, 10)).map(b => b.toString(16).padStart(2, '0')).join(' ')}`);
    }

    // 转发给房间内其他客户端
    // 重要：将 Buffer 转换为字符串，确保浏览器端收到的是 JSON 而不是 Blob
    const messageStr = data.toString();
    const clients = roomClients.get(roomId);
    if (clients) {
      let forwardedCount = 0;
      clients.forEach((client) => {
        if (client !== ws && client.readyState === WebSocket.OPEN) {
          client.send(messageStr);  // 发送字符串而不是 Buffer
          forwardedCount++;
        }
      });
      console.log(`[Server] 转发消息给 ${forwardedCount} 个客户端`);
    }
  });

  // 连接关闭
  ws.on('close', () => {
    const clients = roomClients.get(roomId);
    if (clients) {
      clients.delete(ws);
      console.log(`[Server] 连接关闭: roomId=${roomId}, 剩余连接数: ${clients.size}`);
    }
  });

  ws.on('error', (error) => {
    console.error(`[Server] WebSocket 错误:`, error.message);
  });
});

// ========== 启动服务器 ==========

server.listen(PORT, () => {
  console.log('');
  console.log('╔════════════════════════════════════════════════════════════╗');
  console.log('║                                                            ║');
  console.log('║              Strange Room Signaling Server v3.0            ║');
  console.log('║                                                            ║');
  console.log(`║  HTTP API: http://localhost:${PORT}                        ║`);
  console.log(`║  WebRTC Signaling: ws://localhost:${PORT}/signaling        ║`);
  console.log(`║  Yjs WebSocket: ws://localhost:${PORT}/yjs                 ║`);
  console.log('║                                                            ║');
  console.log('║  功能: 房间管理 | 自包含令牌 | WebRTC + Yjs 信令          ║');
  console.log('║                                                            ║');
  console.log('╚════════════════════════════════════════════════════════════╝');
  console.log('');
  console.log(`[Server] 服务器运行在端口 ${PORT}`);
  console.log('[Server] 按 Ctrl+C 停止服务器');
  console.log('');
});

// 优雅关闭
process.on('SIGTERM', () => {
  console.log('[Server] 收到 SIGTERM 信号，正在关闭...');
  wss.clients.forEach((ws) => ws.close());
  server.close(() => {
    console.log('[Server] 服务器已关闭');
    process.exit(0);
  });
});

process.on('SIGINT', () => {
  console.log('\n[Server] 收到 SIGINT 信号，正在关闭...');
  wss.clients.forEach((ws) => ws.close());
  server.close(() => {
    console.log('[Server] 服务器已关闭');
    process.exit(0);
  });
});
