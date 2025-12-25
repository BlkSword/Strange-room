/**
 * Yjs 协同同步 Hook
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { YjsManager, YjsConfig } from '@/lib/yjs/y-doc';
import { AwarenessState } from '@/types/yjs';

const USER_COLORS = [
  '#FF6B6B', '#4ECDC4', '#45B7D1', '#FFA07A', '#98D8C8',
  '#F7DC6F', '#BB8FCE', '#85C1E2', '#F8B739', '#52C785',
];

export interface UseYjsOptions extends Partial<YjsConfig> {
  peerId?: string; // 从外部传入的 peer ID（基于昵称生成）
  onSynced?: (isSynced: boolean) => void;
  token?: string; // 访问令牌
  encryptionEnabled?: boolean; // 是否启用端到端加密
}

export function useYjs(options: UseYjsOptions = {}) {
  const [isConnected, setIsConnected] = useState(false);
  const [onlineUsers, setOnlineUsers] = useState<AwarenessState[]>([]);
  const [chatMessages, setChatMessages] = useState<any[]>([]);
  const [decryptedMessages, setDecryptedMessages] = useState<any[]>([]);

  const yjsRef = useRef<YjsManager | null>(null);
  // 使用外部传入的 peerId，如果没有则使用临时 ID
  const peerIdRef = useRef<string>(options?.peerId || 'temp_user');

  // 当外部传入的 peerId 变化时，更新 peerIdRef 并同步到 Yjs
  useEffect(() => {
    if (options?.peerId && options.peerId !== 'temp_user' && options.peerId !== peerIdRef.current) {
      peerIdRef.current = options.peerId;
      console.log('[useYjs] peerId 已更新:', options.peerId);

      // 更新 Yjs 中的用户信息
      const yjs = yjsRef.current;
      if (yjs && options.userName) {
        yjs.updateUserInfo(options.userName, options.userColor);
      }
    }
  }, [options?.peerId, options?.userName, options?.userColor]);

  // 初始化 Yjs
  useEffect(() => {
    if (!options.roomId) return;

    // 验证 token 不为空
    if (!options.token || options.token.trim() === '') {
      console.error('[useYjs] Token 为空，跳过 Yjs 初始化');
      return;
    }

    try {
      const config: YjsConfig = {
        roomId: options.roomId,
        userId: peerIdRef.current,
        userName: options.userName || '匿名用户',
        userColor: options.userColor || USER_COLORS[Math.floor(Math.random() * USER_COLORS.length)],
        token: options.token,
        signalingServers: options.signalingServers,
        encryptionKey: options.encryptionKey,
      };

      const yjs = new YjsManager(config);
      yjsRef.current = yjs;

      // 监听连接状态
      if (yjs.provider) {
        yjs.provider.on('sync', (event: any) => {
          console.log('[useYjs] 同步状态:', event);
          setIsConnected(true);
        });

        yjs.provider.on('status', (event: { status: string }) => {
          setIsConnected(event.status === 'connected');
        });
      }

      // 监听聊天消息变化
      yjs.onChatChange(async (messages) => {
        setChatMessages(messages);

        // 如果启用了加密，解密消息
        if (options.encryptionEnabled && yjs.hasEncryption()) {
          const decrypted = await yjs.decryptChatMessages(messages);
          setDecryptedMessages(decrypted);
        } else {
          setDecryptedMessages(messages);
        }
      });

      // 更新在线用户列表 - 监听 awareness 变化
      const updateUsers = () => {
        const users = yjs.getOnlineUsers();
        console.log('[useYjs] 更新在线用户列表:', users.length, '个用户');
        setOnlineUsers(users);
      };

      // 初始更新
      updateUsers();

      // 监听 awareness 变化，实时更新用户列表
      const handleAwarenessChange = () => {
        updateUsers();
      };

      if (yjs.awareness) {
        yjs.awareness.on('change', handleAwarenessChange);
      }

      return () => {
        if (yjs.awareness) {
          yjs.awareness.off('change', handleAwarenessChange);
        }
        yjs.destroy();
      };
    } catch (error) {
      console.error('[useYjs] 初始化错误:', error);
    }
  }, [options.roomId, options.token, options.encryptionKey]);

  // 发送聊天消息
  const sendMessage = useCallback((content: string, type: 'text' | 'image' | 'file' | 'system' = 'text') => {
    const yjs = yjsRef.current;
    if (!yjs) return null;

    // 如果启用了加密，发送加密消息
    if (options.encryptionEnabled && yjs.hasEncryption()) {
      return yjs.sendEncryptedChatMessage(content, type);
    }

    return yjs.sendChatMessage(content, type);
  }, [options.encryptionEnabled]);

  // 获取聊天消息
  const getMessages = useCallback(() => {
    const yjs = yjsRef.current;
    if (!yjs) return [];
    return yjs.getChatMessages();
  }, []);

  // 更新代码
  const updateCode = useCallback((text: string) => {
    const yjs = yjsRef.current;
    if (!yjs) return;
    yjs.updateCode(text);
  }, []);

  // 获取代码
  const getCode = useCallback((): string => {
    const yjs = yjsRef.current;
    if (!yjs) return '';
    return yjs.getCode();
  }, []);

  // 监听代码变化
  const onCodeChange = useCallback((callback: (text: string) => void) => {
    const yjs = yjsRef.current;
    if (!yjs) return;

    yjs.onCodeChange(callback);
  }, []);

  // 更新光标位置
  const updateCursor = useCallback((x: number, y: number, page: 'chat' | 'whiteboard' | 'editor') => {
    const yjs = yjsRef.current;
    if (!yjs) return;
    yjs.updateCursor(x, y, page);
  }, []);

  // 获取其他用户的光标位置
  const getOtherCursors = useCallback((): Map<string, { x: number; y: number; page: string; user: AwarenessState['user'] }> => {
    const yjs = yjsRef.current;
    if (!yjs) return new Map();

    const cursors = new Map();
    const states = yjs.awareness?.getStates() as Map<number, AwarenessState>;

    states?.forEach((state, clientId) => {
      if (state.cursor && state.user?.id !== peerIdRef.current) {
        cursors.set(state.user.id, {
          x: state.cursor.x,
          y: state.cursor.y,
          page: state.cursor.page,
          user: state.user,
        });
      }
    });

    return cursors;
  }, []);

  // 设置房间信息
  const setRoomInfo = useCallback((key: string, value: any) => {
    const yjs = yjsRef.current;
    if (!yjs) return;
    yjs.setRoomInfo(key, value);
  }, []);

  // 获取房间信息
  const getRoomInfo = useCallback((key: string) => {
    const yjs = yjsRef.current;
    if (!yjs) return undefined;
    return yjs.getRoomInfo(key);
  }, []);

  // 清空所有数据
  const clearAllData = useCallback(() => {
    const yjs = yjsRef.current;
    if (!yjs) return;
    yjs.clearAllData();
  }, []);

  // 销毁 Yjs
  const destroy = useCallback(() => {
    const yjs = yjsRef.current;
    if (!yjs) return;
    yjs.destroy();
    yjsRef.current = null;
  }, []);

  // 更新用户信息
  const updateUserInfo = useCallback((userName: string, userColor?: string) => {
    const yjs = yjsRef.current;
    if (!yjs) return;
    yjs.updateUserInfo(userName, userColor);
  }, []);

  // ========== 加密相关方法 ==========

  // 初始化加密（生成新密钥）
  const initEncryption = useCallback(async (): Promise<string | null> => {
    const yjs = yjsRef.current;
    if (!yjs) return null;
    try {
      return await yjs.initEncryption();
    } catch (error) {
      console.error('[useYjs] 初始化加密失败:', error);
      return null;
    }
  }, []);

  // 初始化加密（导入已有密钥）
  const importEncryptionKey = useCallback(async (keyString: string): Promise<boolean> => {
    const yjs = yjsRef.current;
    if (!yjs) return false;
    try {
      await yjs.initEncryption(keyString);
      return true;
    } catch (error) {
      console.error('[useYjs] 导入加密密钥失败:', error);
      return false;
    }
  }, []);

  // 检查是否已启用加密
  const hasEncryption = useCallback((): boolean => {
    const yjs = yjsRef.current;
    if (!yjs) return false;
    return yjs.hasEncryption();
  }, []);

  return {
    // State
    isConnected,
    onlineUsers,
    chatMessages,
    decryptedMessages, // 解密后的消息
    peerId: peerIdRef.current,

    // Yjs 实例
    yjs: yjsRef.current,

    // Actions
    sendMessage,
    getMessages,
    updateCode,
    getCode,
    onCodeChange,
    updateCursor,
    updateUserInfo,
    getOtherCursors,
    setRoomInfo,
    getRoomInfo,
    clearAllData,
    destroy,

    // Encryption Actions
    initEncryption,
    importEncryptionKey,
    hasEncryption,
  };
}
