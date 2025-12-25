/**
 * Yjs 文档管理 - 协同同步核心
 */

import * as Y from 'yjs';
import { WebrtcProvider } from 'y-webrtc';
import { IndexeddbPersistence } from 'y-indexeddb';
import { Y_STORE_KEYS, AwarenessState } from '@/types/yjs';
import { nanoid } from 'nanoid';

export interface YjsConfig {
  roomId: string;
  userId: string;
  userName: string;
  userColor: string;
  token?: string; // 访问令牌
  signalingServers?: string[];
}

export class YjsManager {
  public doc: Y.Doc;
  public provider: WebrtcProvider | null = null;
  public idbPersistence: IndexeddbPersistence | null = null;
  public awareness: any;
  public roomInfoMap: Y.Map<any>;

  private config: YjsConfig;
  private destroyCallbacks: Set<() => void> = new Set();

  // 数据片段
  public chatArray: Y.Array<any>;
  public canvasFragment: Y.XmlFragment;
  public codeText: Y.Text;
  public filesMap: Y.Map<any>;
  public roomInfoMap: Y.Map<any>;

  constructor(config: YjsConfig) {
    this.config = config;

    // 创建 Yjs 文档
    this.doc = new Y.Doc();

    // 获取数据片段
    this.chatArray = this.doc.getArray(Y_STORE_KEYS.CHAT);
    this.canvasFragment = this.doc.getXmlFragment(Y_STORE_KEYS.CANVAS);
    this.codeText = this.doc.getText(Y_STORE_KEYS.CODE);
    this.filesMap = this.doc.getMap(Y_STORE_KEYS.FILES);
    this.roomInfoMap = this.doc.getMap(Y_STORE_KEYS.ROOM_INFO);

    // 初始化 IndexedDB 持久化
    this.initPersistence();

    // 初始化 WebRTC 提供者
    this.initProvider();
  }

  /**
   * 初始化 IndexedDB 持久化
   */
  private initPersistence() {
    try {
      this.idbPersistence = new IndexeddbPersistence(
        `strange-room-${this.config.roomId}`,
        this.doc
      );

      this.idbPersistence.on('sync', () => {
        console.log('[Yjs] 数据已从 IndexedDB 同步');
      });
    } catch (error) {
      console.error('[Yjs] IndexedDB 初始化失败:', error);
    }
  }

  /**
   * 初始化 WebRTC 提供者
   */
  private initProvider() {
    // 构建 WebSocket URL（带 roomId 和 token 参数）
    const baseUrl = this.config.signalingServers?.[0] || 'ws://localhost:9001';
    const wsUrl = `${baseUrl}/signaling?roomId=${encodeURIComponent(this.config.roomId)}&token=${encodeURIComponent(this.config.token || '')}`;

    console.log('[Yjs] WebSocket URL:', wsUrl);

    this.provider = new WebrtcProvider(
      this.config.roomId,
      this.doc,
      {
        signaling: [wsUrl],
        maxConns: 20,
        filterBcConns: true,
        peerOpts: {},
        // 启用本地网络发现
        rtcConfig: {
          iceServers: [
            { urls: 'stun:stun.l.google.com:19302' },
            { urls: 'stun:stun1.l.google.com:19302' },
          ]
        }
      }
    );

    this.awareness = this.provider.awareness;

    console.log('[Yjs] 房间 ID:', this.config.roomId);
    console.log('[Yjs] 用户 ID:', this.config.userId);
    console.log('[Yjs] WebSocket URL:', wsUrl);
    console.log('[Yjs] Token:', this.config.token?.substring(0, 8) + '...');

    // 设置当前用户状态
    this.setAwarenessState({
      user: {
        id: this.config.userId,
        name: this.config.userName,
        color: this.config.userColor,
      },
    });

    // 监听其他用户变化
    this.awareness.on('change', () => {
      const states = this.awareness.getStates() as Map<number, AwarenessState>;
      console.log('[Yjs] Awareness 变化, 在线用户数:', states.size);
      states.forEach((state, clientId) => {
        console.log('[Yjs] 用户:', state.user?.name, 'ID:', state.user?.id);
      });
      this.onAwarenessChange(states);
    });

    // 监听连接状态
    this.provider.on('synced', ({ synced }: { synced: boolean }) => {
      console.log('[Yjs] 同步状态:', synced);
    });

    // 监听 WebRTC 连接变化
    this.provider.on('connection-error', (error: any) => {
      console.error('[Yjs] WebRTC 连接错误:', error);
    });

    this.provider.on('peers', (event: any) => {
      console.log('[Yjs] 对等节点变化:', {
        removed: event.removed,
        added: event.added,
        peers: event.peers,
        webrtcPeers: this.provider?.webrtcPeers?.size || 0
      });
    });
  }

  /**
   * 设置用户状态
   */
  setAwarenessState(state: Partial<AwarenessState>) {
    if (!this.awareness) return;

    const current = this.awareness.getLocalState() as AwarenessState;
    this.awareness.setLocalStateField('user', {
      ...current?.user,
      ...state.user,
    });

    if (state.cursor) {
      this.awareness.setLocalStateField('cursor', state.cursor);
    }
  }

  /**
   * 更新光标位置
   */
  updateCursor(x: number, y: number, page: 'chat' | 'whiteboard' | 'editor') {
    this.setAwarenessState({
      cursor: { x, y, page },
    });
  }

  /**
   * 更新用户信息（昵称等）
   */
  updateUserInfo(userName: string, userColor?: string) {
    this.config.userName = userName;
    if (userColor) {
      this.config.userColor = userColor;
    }
    this.setAwarenessState({
      user: {
        id: this.config.userId,
        name: userName,
        color: userColor || this.config.userColor,
      },
    });
    console.log('[Yjs] 用户信息已更新:', userName);
  }

  /**
   * 获取所有在线用户
   */
  getOnlineUsers(): AwarenessState[] {
    if (!this.awareness) {
      console.log('[Yjs] getOnlineUsers: awareness 不存在');
      return [];
    }

    const states: AwarenessState[] = [];
    this.awareness.getStates().forEach((state: AwarenessState) => {
      if (state.user) {
        states.push(state);
      }
    });

    console.log('[Yjs] getOnlineUsers: 返回', states.length, '个用户');
    return states;
  }

  /**
   * 发送聊天消息
   */
  sendChatMessage(content: string, type: 'text' | 'image' | 'file' | 'system' = 'text') {
    // 获取当前 awareness 中的用户信息（可能已被 updateUserInfo 更新）
    const currentState = this.awareness?.getLocalState() as AwarenessState;
    const senderName = currentState?.user?.name || this.config.userName;

    const message = {
      id: nanoid(),
      type,
      content,
      senderId: this.config.userId,
      senderName: senderName,
      timestamp: Date.now(),
    };

    this.doc.transact(() => {
      this.chatArray.push([message]);
    });

    return message;
  }

  /**
   * 获取聊天消息
   */
  getChatMessages() {
    return this.chatArray.toArray();
  }

  /**
   * 监听聊天变化
   */
  onChatChange(callback: (messages: any[]) => void) {
    this.chatArray.observe(() => {
      callback(this.chatArray.toArray());
    });
  }

  /**
   * 监听代码变化
   */
  onCodeChange(callback: (text: string) => void) {
    this.codeText.observe(() => {
      callback(this.codeText.toString());
    });
  }

  /**
   * 更新代码
   */
  updateCode(text: string) {
    this.doc.transact(() => {
      this.codeText.delete(0, this.codeText.length);
      this.codeText.insert(0, text);
    });
  }

  /**
   * 获取代码内容
   */
  getCode(): string {
    return this.codeText.toString();
  }

  /**
   * 监听房间信息变化
   */
  onRoomInfoChange(callback: (info: Y.Map<any>) => void) {
    this.roomInfoMap.observe(() => {
      callback(this.roomInfoMap);
    });
  }

  /**
   * 设置房间信息
   */
  setRoomInfo(key: string, value: any) {
    console.log('[Yjs] setRoomInfo:', key, value?.substring?.(0, 50) || value);
    this.doc.transact(() => {
      this.roomInfoMap.set(key, value);
    });
    console.log('[Yjs] setRoomInfo 完成，当前 roomInfoMap 大小:', this.roomInfoMap.size);
  }

  /**
   * 获取房间信息
   */
  getRoomInfo(key: string) {
    return this.roomInfoMap.get(key);
  }

  /**
   * Awareness 变化回调
   */
  private onAwarenessChange(states: Map<number, AwarenessState>) {
    // 可以在这里处理用户上线/下线事件
  }

  /**
   * 注册销毁回调
   */
  onDestroy(callback: () => void) {
    this.destroyCallbacks.add(callback);
  }

  /**
   * 销毁 Yjs 实例
   */
  destroy() {
    // 触发销毁回调
    this.destroyCallbacks.forEach(cb => cb());

    // 清空 IndexedDB
    if (this.idbPersistence) {
      this.idbPersistence.destroy();
    }

    // 断开 WebRTC 连接
    if (this.provider) {
      this.provider.destroy();
    }

    // 销毁文档
    this.doc.destroy();

    console.log('[Yjs] 实例已销毁');
  }

  /**
   * 清空房间数据 (用于销毁房间)
   */
  clearAllData() {
    this.doc.transact(() => {
      // 清空所有数据
      this.chatArray.delete(0, this.chatArray.length);
      this.canvasFragment.delete(0, this.canvasFragment.length);
      this.codeText.delete(0, this.codeText.length);
      this.filesMap.clear();
      this.roomInfoMap.clear();
    });
  }
}
