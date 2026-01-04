/**
 * 房间数据导出/还原核心模块
 */

import * as Y from 'yjs';
import { RoomExportData, ExportOptions, ImportResult, EncryptedExportData } from '@/types/export';
import { YjsManager } from '@/lib/yjs/y-doc';
import { encryptExportData, decryptExportData } from './crypto-utils';
import { Room } from '@/types/room';

/**
 * 导出房间完整数据
 */
export async function exportRoomData(
  yjsManager: YjsManager,
  room: Room,
  roomId: string,
  options: ExportOptions = {}
): Promise<RoomExportData> {
  const {
    includeToken = true,
    includeEncryptionKey = true,
  } = options;

  // 获取 Yjs 文档状态并转换为 Base64
  const stateAsUpdate = Y.encodeStateAsUpdate(yjsManager.doc);
  const stateAsUpdateBase64 = btoa(String.fromCharCode(...stateAsUpdate));

  // 导出 Yjs 数据
  const yjsData = {
    stateAsUpdate: stateAsUpdateBase64,
    chat: yjsManager.chatArray.toArray(),
    code: yjsManager.codeText.toString(),
    canvas: yjsManager.canvasFragment.toString(),
    files: Object.fromEntries(yjsManager.filesMap.entries()),
    roomInfo: Object.fromEntries(yjsManager.roomInfoMap.entries()),
  };

  // 获取用户数据（localStorage）
  const userData: {
    token?: string;
    encryptionKey?: string;
    encryptionEnabled?: boolean;
  } = {};

  if (includeToken) {
    userData.token = localStorage.getItem(`room-token-${roomId}`) || undefined;
  }

  if (includeEncryptionKey) {
    userData.encryptionKey = localStorage.getItem(`room-encryption-key-${roomId}`) || undefined;
    userData.encryptionEnabled = localStorage.getItem(`room-encryption-enabled-${roomId}`) === 'true';
  }

  // 构建导出数据
  const exportData: RoomExportData = {
    version: '1.0',
    exportedAt: Date.now(),
    roomId,
    roomName: room.name,
    metadata: {
      createdAt: room.createdAt,
      ttl: room.ttl,
      expiresAt: room.expiresAt,
      idleTimeout: room.idleTimeout,
      lastActiveAt: room.lastActiveAt,
      creatorPeerId: room.creatorPeerId,
    },
    yjsData,
    userData,
  };

  return exportData;
}

/**
 * 导出房间数据并加密（如果提供了密码）
 */
export async function exportRoomDataWithPassword(
  yjsManager: YjsManager,
  room: Room,
  roomId: string,
  password?: string,
  options: ExportOptions = {}
): Promise<RoomExportData | EncryptedExportData> {
  const exportData = await exportRoomData(yjsManager, room, roomId, options);

  if (password) {
    return encryptExportData(exportData, password);
  }

  return exportData;
}

/**
 * 还原房间数据
 */
export async function importRoomData(
  yjsManager: YjsManager,
  importData: RoomExportData | EncryptedExportData,
  password?: string,
  roomId?: string
): Promise<ImportResult> {
  const warnings: string[] = [];

  try {
    // 如果是加密数据，先解密
    let data: RoomExportData;
    if ('encrypted' in importData && importData.encrypted) {
      if (!password) {
        return {
          success: false,
          error: '需要提供密码才能解密数据',
        };
      }

      try {
        data = decryptExportData(importData as EncryptedExportData, password);
      } catch (error) {
        return {
          success: false,
          error: '解密失败：密码错误或数据损坏',
        };
      }
    } else {
      data = importData as RoomExportData;
    }

    // 验证版本兼容性
    if (data.version !== '1.0') {
      warnings.push(`导出数据版本 ${data.version} 可能与当前版本不完全兼容`);
    }

    // 验证房间ID（如果提供了）
    if (roomId && data.roomId !== roomId) {
      warnings.push(`导入数据的房间ID (${data.roomId}) 与当前房间ID (${roomId}) 不匹配`);
    }

    // 还原 Yjs 数据
    try {
      // 将 Base64 转换回 Uint8Array
      const binaryString = atob(data.yjsData.stateAsUpdate);
      const bytes = new Uint8Array(binaryString.length);
      for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }
      // 应用完整状态更新
      Y.applyUpdate(yjsManager.doc, bytes);

      // 同步各个数据片段（状态更新后应该已经自动同步，这里作为额外保障）
      // 注意：不需要手动设置，因为 applyUpdate 会完整恢复文档状态
    } catch (error) {
      warnings.push('Yjs 数据还原部分失败：' + (error as Error).message);
    }

    // 还原用户数据到 localStorage
    if (data.userData.token) {
      const targetRoomId = roomId || data.roomId;
      localStorage.setItem(`room-token-${targetRoomId}`, data.userData.token);
    }

    if (data.userData.encryptionKey) {
      const targetRoomId = roomId || data.roomId;
      localStorage.setItem(`room-encryption-key-${targetRoomId}`, data.userData.encryptionKey);
    }

    if (data.userData.encryptionEnabled !== undefined) {
      const targetRoomId = roomId || data.roomId;
      localStorage.setItem(`room-encryption-enabled-${targetRoomId}`, String(data.userData.encryptionEnabled));
    }

    return {
      success: true,
      data,
      warnings: warnings.length > 0 ? warnings : undefined,
    };
  } catch (error) {
    return {
      success: false,
      error: '导入数据失败：' + (error as Error).message,
    };
  }
}

/**
 * 下载导出数据为文件
 */
export function downloadExportData(
  data: RoomExportData | EncryptedExportData,
  filename?: string
): void {
  const jsonString = JSON.stringify(data, null, 2);
  const blob = new Blob([jsonString], { type: 'application/json' });
  const url = URL.createObjectURL(blob);

  const link = document.createElement('a');
  link.href = url;
  link.download = filename || `room-export-${Date.now()}.json`;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);

  URL.revokeObjectURL(url);
}

/**
 * 读取导出文件
 */
export function readExportFile(file: File): Promise<RoomExportData | EncryptedExportData> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = (e) => {
      try {
        const content = e.target?.result as string;
        const data = JSON.parse(content);
        resolve(data);
      } catch (error) {
        reject(new Error('文件解析失败：' + (error as Error).message));
      }
    };

    reader.onerror = () => {
      reject(new Error('文件读取失败'));
    };

    reader.readAsText(file);
  });
}

/**
 * 验证导出数据格式
 */
export function validateExportData(data: any): data is RoomExportData | EncryptedExportData {
  if (!data || typeof data !== 'object') {
    return false;
  }

  // 检查是否是加密数据
  if (data.encrypted === true) {
    return (
      typeof data.version === 'string' &&
      typeof data.iv === 'string' &&
      typeof data.salt === 'string' &&
      typeof data.data === 'string'
    );
  }

  // 检查是否是未加密数据
  return (
    typeof data.version === 'string' &&
    typeof data.exportedAt === 'number' &&
    typeof data.roomId === 'string' &&
    typeof data.roomName === 'string' &&
    data.metadata &&
    data.yjsData &&
    data.userData
  );
}

/**
 * 获取导出文件信息（不解密）
 */
export function getExportInfo(data: RoomExportData | EncryptedExportData): {
  version: string;
  encrypted: boolean;
  roomId?: string;
  roomName?: string;
  exportedAt?: number;
} {
  if ('encrypted' in data && data.encrypted) {
    return {
      version: data.version,
      encrypted: true,
    };
  }

  const exportData = data as RoomExportData;
  return {
    version: exportData.version,
    encrypted: false,
    roomId: exportData.roomId,
    roomName: exportData.roomName,
    exportedAt: exportData.exportedAt,
  };
}
