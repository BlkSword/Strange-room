/**
 * Yjs 文档管理 - 协同同步核心
 */

import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';
import { IndexeddbPersistence } from 'y-indexeddb';
import { Y_STORE_KEYS, AwarenessState } from '@/types/yjs';
import { nanoid } from 'nanoid';
import { RoomKeyManager } from '@/lib/crypto/e2e-crypto';

export interface YjsConfig {
  roomId: string;
  userId: string;
  userName: string;
  userColor: string;
  token?: string; // 访问令牌
  signalingServers?: string[];
  encryptionKey?: string; // 房间加密密钥（可选，用于加入已有房间）
}

export class YjsManager {
  public doc: Y.Doc;
  public provider: WebsocketProvider | null = null;
  public idbPersistence: IndexeddbPersistence | null = null;
  public awareness: any;
  public roomInfoMap: Y.Map<any>;
  public shouldReconnect: boolean = true; // 控制是否应该重连

  private config: YjsConfig;
  private destroyCallbacks: Set<() => void> = new Set();
  private keyManager: RoomKeyManager;
  private encryptionEnabled: boolean = false;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;

  // 数据片段
  public chatArray: Y.Array<any>;
  public canvasFragment: Y.XmlFragment;
  public codeText: Y.Text;
  public filesMap: Y.Map<any>;

  constructor(config: YjsConfig) {
    this.config = config;
    this.keyManager = new RoomKeyManager();

    // 创建 Yjs 文档
    this.doc = new Y.Doc();

    // 获取数据片段
    this.chatArray = this.doc.getArray(Y_STORE_KEYS.CHAT);
    this.canvasFragment = this.doc.getXmlFragment(Y_STORE_KEYS.CANVAS);
    this.codeText = this.doc.getText(Y_STORE_KEYS.CODE);
    this.filesMap = this.doc.getMap(Y_STORE_KEYS.FILES);
    this.roomInfoMap = this.doc.getMap(Y_STORE_KEYS.ROOM_INFO);

    // 初始化加密（如果提供了密钥）
    if (config.encryptionKey) {
      this.initEncryption(config.encryptionKey);
    }

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
   * 根据 HTTP origin 构建 WebSocket URL
   * http://192.168.1.100:3000 -> ws://192.168.1.100:9001
   * https://example.com -> wss://example.com:9001
   */
  private buildWebSocketUrl(origin: string): string {
    try {
      const url = new URL(origin);
      const protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
      // 默认信令服务器端口为 9001
      return `${protocol}//${url.hostname}:9001`;
    } catch {
      return 'ws://localhost:9001';
    }
  }

  /**
   * 初始化 WebSocket 提供者（使用 y-websocket）
   */
  private initProvider() {
    // 动态构建 WebSocket URL：根据当前页面地址自动适配
    const origin = typeof window !== 'undefined' ? window.location.origin : '';
    const baseUrl = this.config.signalingServers?.[0] || this.buildWebSocketUrl(origin);
    // y-websocket 会自动在 URL 后面添加 /roomId
    const wsUrl = `${baseUrl}/yjs`;

    console.log('[Yjs] WebSocket URL:', wsUrl);
    console.log('[Yjs] 当前页面 origin:', origin);

    try {
      this.provider = new WebsocketProvider(
        wsUrl,
        this.config.roomId,
        this.doc,
        {
          connect: true,
          // 通过 params 传递 token
          params: {
            token: this.config.token || '',
          },
        }
      );
      console.log('[Yjs] WebsocketProvider 创建成功');
    } catch (error) {
      console.error('[Yjs] WebsocketProvider 创建失败:', error);
      throw error;
    }

    // 暴露到全局用于调试
    if (typeof window !== 'undefined') {
      (window as any).__yjsProvider__ = this.provider;
      console.log('[Yjs] Provider 已暴露到 window.__yjsProvider__');
    }

    this.awareness = this.provider.awareness;

    console.log('[Yjs] ========== WebSocket 配置 ==========');
    console.log('[Yjs] 房间 ID:', this.config.roomId);
    console.log('[Yjs] 用户 ID:', this.config.userId);
    console.log('[Yjs] WebSocket URL:', wsUrl);
    console.log('[Yjs] Token:', this.config.token?.substring(0, 8) + '...');
    console.log('[Yjs] =====================================');

    // 监听 WebSocket 连接状态
    this.provider.on('status', (event: { status: string }) => {
      console.log('[Yjs] WebSocket 状态:', event.status);
      // status: 'connecting' | 'connected' | 'disconnected'
    });

    this.provider.on('connection-error', (error: any) => {
      console.error('[Yjs] WebSocket 连接错误:', error);
    });

    this.provider.on('sync', (event: { syncStep: number }) => {
      console.log('[Yjs] 同步进度:', event.syncStep);
    });

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

    // 监听 WebSocket 连接变化
    this.provider.ws?.addEventListener('open', () => {
      console.log('[Yjs] WebSocket 连接已建立');
    });

    this.provider.ws?.addEventListener('close', (event: CloseEvent) => {
      console.log('[Yjs] WebSocket 连接已关闭, code:', event.code, 'reason:', event.reason);

      // 如果服务器明确表示房间不存在（1008 状态码），停止重连
      if (event.code === 1008 && event.reason === 'room_not_found') {
        console.warn('[Yjs] 房间不存在，停止重连');
        this.shouldReconnect = false;
        // 停止 WebsocketProvider 的自动重连
        if (this.provider) {
          this.provider.destroy();
          this.provider = null;
        }
      }
    });

    this.provider.ws?.addEventListener('error', (error) => {
      console.error('[Yjs] WebSocket 错误:', error);
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
   * @returns 取消订阅函数
   */
  onChatChange(callback: (messages: any[]) => void): () => void {
    const handler = () => {
      callback(this.chatArray.toArray());
    };
    this.chatArray.observe(handler);
    // 返回取消订阅函数
    return () => {
      this.chatArray.unobserve(handler);
    };
  }

  /**
   * 监听代码变化
   * @returns 取消订阅函数
   */
  onCodeChange(callback: (text: string) => void): () => void {
    const handler = () => {
      callback(this.codeText.toString());
    };
    this.codeText.observe(handler);
    // 返回取消订阅函数
    return () => {
      this.codeText.unobserve(handler);
    };
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

    // 断开 WebSocket 连接
    if (this.provider) {
      this.provider.destroy();
    }

    // 销毁文档
    this.doc.destroy();

    console.log('[Yjs] 实例已销毁');
  }

  // ========== 端到端加密相关方法 ==========

  /**
   * 初始化加密 - 生成新密钥
   */
  async initEncryption(): Promise<string> {
    const result = await this.keyManager.createRoom();
    this.encryptionEnabled = true;
    console.log('[Yjs] E2E 加密已启用，密钥 ID:', result.keyId);
    return result.keyString;
  }

  /**
   * 初始化加密 - 导入已有密钥
   */
  async initEncryption(keyString: string): Promise<void> {
    await this.keyManager.joinRoom(keyString);
    this.encryptionEnabled = true;
    console.log('[Yjs] E2E 加密已启用（导入密钥）');
  }

  /**
   * 检查是否已启用加密
   */
  hasEncryption(): boolean {
    return this.encryptionEnabled && this.keyManager.hasKey();
  }

  /**
   * 发送加密聊天消息
   */
  async sendEncryptedChatMessage(content: string, type: 'text' | 'image' | 'file' | 'system' = 'text') {
    if (!this.encryptionEnabled) {
      console.warn('[Yjs] 加密未启用，发送未加密消息');
      return this.sendChatMessage(content, type);
    }

    // 获取当前 awareness 中的用户信息
    const currentState = this.awareness?.getLocalState() as AwarenessState;
    const senderName = currentState?.user?.name || this.config.userName;

    // 加密消息内容
    const encryptedResult = await this.keyManager.encryptMessage({
      content,
      type,
      senderId: this.config.userId,
      senderName,
    });

    // 创建包含加密数据的消息
    const message = {
      id: nanoid(),
      type: 'encrypted', // 标记为加密消息
      encrypted: encryptedResult.encrypted,
      keyId: encryptedResult.keyId,
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
   * 解密聊天消息
   */
  async decryptChatMessage(message: any): Promise<any> {
    // 如果不是加密消息，直接返回
    if (message.type !== 'encrypted') {
      return message;
    }

    if (!this.encryptionEnabled) {
      console.warn('[Yjs] 加密未启用，无法解密消息');
      return {
        ...message,
        content: '[加密消息 - 无法解密]',
        type: 'system',
      };
    }

    try {
      const decrypted = await this.keyManager.decryptMessage(message.encrypted);
      if (decrypted) {
        return {
          id: message.id,
          ...decrypted,
          senderId: message.senderId,
          senderName: message.senderName,
          timestamp: message.timestamp,
        };
      }
      return {
        ...message,
        content: '[解密失败]',
        type: 'system',
      };
    } catch (error) {
      console.error('[Yjs] 解密消息失败:', error);
      return {
        ...message,
        content: '[解密失败]',
        type: 'system',
      };
    }
  }

  /**
   * 批量解密聊天消息
   */
  async decryptChatMessages(messages: any[]): Promise<any[]> {
    const decryptedPromises = messages.map(msg => this.decryptChatMessage(msg));
    return await Promise.all(decryptedPromises);
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
