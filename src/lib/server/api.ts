/**
 * 信令服务器 API 客户端
 */

const SERVER_URL = process.env.NEXT_PUBLIC_SIGNALING_SERVER || 'http://localhost:9001';

export interface CreateRoomResponse {
  success: boolean;
  roomId?: string;
  expiresAt?: number;
  error?: string;
}

export interface CheckRoomResponse {
  exists: boolean;
  room?: {
    createdAt: number;
    expiresAt: number;
    creator: string;
  };
}

export interface GenerateTokenResponse {
  success: boolean;
  token?: string;
  expiresAt?: number;
  error?: string;
}

export interface ValidateTokenResponse {
  valid: boolean;
  roomId?: string;
  error?: string;
}

/**
 * 创建房间
 */
export async function createRoom(ttl: number, creatorName: string): Promise<CreateRoomResponse> {
  try {
    const response = await fetch(`${SERVER_URL}/api/room/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ttl, creatorName }),
    });

    const data = await response.json();
    return data;
  } catch (error) {
    console.error('[API] 创建房间失败:', error);
    return { success: false, error: 'Network error' };
  }
}

/**
 * 检查房间是否存在
 */
export async function checkRoom(roomId: string): Promise<CheckRoomResponse> {
  try {
    const response = await fetch(`${SERVER_URL}/api/room/check/${roomId}`);
    const data = await response.json();
    return data;
  } catch (error) {
    console.error('[API] 检查房间失败:', error);
    return { exists: false };
  }
}

/**
 * 生成访问令牌
 */
export async function generateToken(roomId: string): Promise<GenerateTokenResponse> {
  try {
    const response = await fetch(`${SERVER_URL}/api/token/generate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ roomId }),
    });

    const data = await response.json();
    return data;
  } catch (error) {
    console.error('[API] 生成令牌失败:', error);
    return { success: false, error: 'Network error' };
  }
}

/**
 * 验证访问令牌
 */
export async function validateToken(token: string): Promise<ValidateTokenResponse> {
  try {
    const response = await fetch(`${SERVER_URL}/api/token/validate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token }),
    });

    const data = await response.json();
    return data;
  } catch (error) {
    console.error('[API] 验证令牌失败:', error);
    return { valid: false, error: 'Network error' };
  }
}

/**
 * 健康检查
 */
export async function healthCheck(): Promise<any> {
  try {
    const response = await fetch(`${SERVER_URL}/health`);
    return await response.json();
  } catch (error) {
    return null;
  }
}
