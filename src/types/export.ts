/**
 * 房间导出/导入类型定义
 */

import * as Y from 'yjs';

/**
 * 房间导出数据结构（完整版）
 */
export interface RoomExportData {
  version: string; // 导出数据格式版本
  exportedAt: number; // 导出时间戳
  roomId: string; // 房间ID
  roomName: string; // 房间名称

  // 房间元数据
  metadata: {
    createdAt: number; // 创建时间
    ttl: number; // 存活时间（小时）
    expiresAt: number; // 过期时间
    idleTimeout: number; // 空闲超时（分钟）
    lastActiveAt: number; // 最后活跃时间
    creatorPeerId: string; // 创建者Peer ID
  };

  // Yjs 协同数据
  yjsData: {
    stateAsUpdate: string; // Yjs 文档完整状态（Base64 编码）
    chat: any[]; // 聊天消息
    code: string; // 代码内容
    canvas: string; // 白板数据（XML字符串）
    files: Record<string, any>; // 文件数据
    roomInfo: Record<string, any>; // 房间信息
  };

  // 用户数据（localStorage）
  userData: {
    token?: string; // 访问令牌
    encryptionKey?: string; // 加密密钥（如果启用了加密）
    encryptionEnabled?: boolean; // 是否启用加密
  };
}

/**
 * 加密后的导出数据
 */
export interface EncryptedExportData {
  version: string;
  encrypted: boolean;
  iv: string; // 初始化向量（Base64）
  salt: string; // 盐值（Base64）
  data: string; // 加密后的数据（Base64）
}

/**
 * 导出配置
 */
export interface ExportOptions {
  password?: string; // 加密密码（可选，不设置则不加密）
  includeToken?: boolean; // 是否包含访问令牌
  includeEncryptionKey?: boolean; // 是否包含加密密钥
}

/**
 * 导入结果
 */
export interface ImportResult {
  success: boolean;
  error?: string;
  data?: RoomExportData;
  warnings?: string[]; // 警告信息（如部分数据无法恢复）
}
