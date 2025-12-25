/**
 * 房间类型定义
 */

export type RoomTTL = 1 | 6 | 24 | 48 | 168; // 小时: 1h, 6h, 24h, 48h, 7天

export interface Room {
  id: string;
  name: string;
  createdAt: number;
  ttl: RoomTTL;
  expiresAt: number;
  creatorPeerId: string;
  peers: Map<string, PeerInfo>;
  destroyed: boolean;
}

export interface PeerInfo {
  id: string;
  nickname: string;
  color: string;
  joinedAt: number;
  isCreator: boolean;
  isOnline: boolean;
  cursor?: CursorPosition;
}

export interface CursorPosition {
  x: number;
  y: number;
  page: 'chat' | 'whiteboard' | 'editor';
}

export interface InviteToken {
  token: string;
  roomId: string;
  createdAt: number;
  usedAt?: number;
  maxUses: number;
}

export interface RoomMessage {
  id: string;
  type: 'text' | 'image' | 'file' | 'system';
  content: string;
  senderId: string;
  senderName: string;
  timestamp: number;
  metadata?: {
    fileSize?: number;
    fileName?: string;
    fileType?: string;
  };
}

export interface CanvasOperation {
  type: 'line' | 'rect' | 'circle' | 'text' | 'eraser';
  points: [number, number][];
  color: string;
  width: number;
  filled?: boolean;
  text?: string;
}

export interface FileTransfer {
  id: string;
  name: string;
  size: number;
  type: string;
  data: ArrayBuffer;
  senderId: string;
  timestamp: number;
}
