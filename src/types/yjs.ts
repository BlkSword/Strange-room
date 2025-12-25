/**
 * Yjs 协同同步类型定义
 */

import * as Y from 'yjs';

// Yjs 文档片段类型
export type YDocFragment = Y.XmlFragment | Y.Text | Y.Array<any> | Y.Map<any>;

// 用户状态 (Awareness)
export interface AwarenessState {
  user: {
    id: string;
    name: string;
    color: string;
  };
  cursor?: {
    x: number;
    y: number;
    page: 'chat' | 'whiteboard' | 'editor';
  };
  selection?: {
    from: number;
    to: number;
  };
}

// Yjs 存储键名
export const Y_STORE_KEYS = {
  CHAT: 'chat',
  CANVAS: 'canvas',
  CODE: 'code',
  FILES: 'files',
  ROOM_INFO: 'roomInfo',
} as const;

// Yjs 事件类型
export interface YjsEventMap {
  'update': (update: Uint8Array, origin: any) => void;
  'sync': (isSynced: boolean) => void;
  'destroy': () => void;
}
