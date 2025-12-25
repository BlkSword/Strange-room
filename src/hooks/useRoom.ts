/**
 * 房间管理 Hook
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { RoomManager } from '@/lib/room/room-manager';
import { Room, RoomTTL, PeerInfo } from '@/types/room';

const USER_COLORS = [
  '#FF6B6B', '#4ECDC4', '#45B7D1', '#FFA07A', '#98D8C8',
  '#F7DC6F', '#BB8FCE', '#85C1E2', '#F8B739', '#52C785',
];

export interface UseRoomOptions {
  peerId?: string; // 从外部传入的 peer ID（基于昵称生成）
  onRoomDestroyed?: (reason: 'expired' | 'creator_left' | 'manual') => void;
}

export function useRoom(options?: UseRoomOptions) {
  const [room, setRoom] = useState<Room | null>(null);
  const [remainingTime, setRemainingTime] = useState<number>(0);
  const [isCreator, setIsCreator] = useState(false);

  const roomManagerRef = useRef<RoomManager | null>(null);
  // 使用外部传入的 peerId，如果没有则使用临时 ID
  const peerIdRef = useRef<string>(options?.peerId || 'temp_user');

  // 当外部传入的 peerId 变化时，更新 peerIdRef
  useEffect(() => {
    if (options?.peerId && options.peerId !== 'temp_user') {
      peerIdRef.current = options.peerId;
      console.log('[useRoom] peerId 已更新:', options.peerId);
    }
  }, [options?.peerId]);

  // 初始化房间管理器
  useEffect(() => {
    roomManagerRef.current = new RoomManager();

    const manager = roomManagerRef.current;

    // 监听倒计时
    manager.onTick((remainingMs) => {
      setRemainingTime(remainingMs);
    });

    // 监听房间销毁
    manager.onDestroy((reason) => {
      const currentRoom = manager.getRoom();
      if (currentRoom?.destroyed) {
        options?.onRoomDestroyed?.(reason);
        setRoom(currentRoom);
      }
    });

    return () => {
      manager.dispose();
    };
  }, []);

  // 创建房间
  const createRoom = useCallback((ttl: RoomTTL, nickname: string) => {
    const manager = roomManagerRef.current;
    if (!manager) return null;

    const color = USER_COLORS[Math.floor(Math.random() * USER_COLORS.length)];
    const newRoom = manager.createRoom({
      ttl,
      creatorNickname: nickname,
      creatorColor: color,
    });

    setRoom(newRoom);
    setRemainingTime(manager.getRemainingTime());
    setIsCreator(true);

    return newRoom;
  }, []);

  // 加入房间
  const joinRoom = useCallback((existingRoom: Room, nickname: string, isCreator: boolean = false) => {
    console.log('[useRoom] joinRoom 被调用, isCreator:', isCreator, 'nickname:', nickname);
    const manager = roomManagerRef.current;
    if (!manager) return null;

    const color = USER_COLORS[Math.floor(Math.random() * USER_COLORS.length)];
    const peer = manager.joinRoom(existingRoom, {
      id: peerIdRef.current,
      nickname,
      color,
      isOnline: true,
      isCreator, // 传入创建者标识
    });

    setRoom(existingRoom);
    setRemainingTime(manager.getRemainingTime());
    console.log('[useRoom] 设置 isCreator 为:', isCreator);
    setIsCreator(isCreator);

    return peer;
  }, []);

  // 离开房间
  const leaveRoom = useCallback(() => {
    const manager = roomManagerRef.current;
    if (!manager) return;

    manager.leaveRoom(peerIdRef.current);
    setRoom(null);
    setRemainingTime(0);
    setIsCreator(false);
  }, []);

  // 销毁房间（仅创建者）
  const destroyRoom = useCallback(() => {
    const manager = roomManagerRef.current;
    if (!manager) return;

    // 双重检查：只有创建者才能销毁房间
    if (!isCreator) {
      console.error('[Room] 权限拒绝：只有创建者才能销毁房间');
      return false;
    }

    manager.destroyRoom('manual');
    return true;
  }, [isCreator]);

  // 踢出用户
  const kickPeer = useCallback((targetPeerId: string) => {
    const manager = roomManagerRef.current;
    if (!manager || !isCreator) return false;

    return manager.kickPeer(peerIdRef.current, targetPeerId);
  }, [isCreator]);

  // 更新光标位置
  const updateCursor = useCallback((x: number, y: number, page: 'chat' | 'whiteboard' | 'editor') => {
    const manager = roomManagerRef.current;
    if (!manager) return;

    manager.updatePeerCursor(peerIdRef.current, { x, y, page });
  }, []);

  // 格式化剩余时间
  const formatRemainingTime = useCallback((ms: number): string => {
    if (ms <= 0) return '已过期';

    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) {
      return `${days}:${String(hours % 24).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`;
    } else if (hours > 0) {
      return `${String(hours).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
    } else {
      return `${String(minutes).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
    }
  }, []);

  // 获取剩余时间百分比
  const getRemainingPercent = useCallback((): number => {
    const manager = roomManagerRef.current;
    if (!manager) return 0;
    return manager.getRemainingTimePercent();
  }, []);

  // 是否即将过期
  const isNearExpiry = useCallback((): boolean => {
    const manager = roomManagerRef.current;
    if (!manager) return false;
    return manager.isNearExpiry();
  }, []);

  // 是否紧急过期
  const isUrgentExpiry = useCallback((): boolean => {
    const manager = roomManagerRef.current;
    if (!manager) return false;
    return manager.isUrgentExpiry();
  }, []);

  // 获取在线用户
  const getOnlinePeers = useCallback((): PeerInfo[] => {
    const manager = roomManagerRef.current;
    if (!manager) return [];
    return manager.getOnlinePeers();
  }, []);

  return {
    room,
    remainingTime,
    isCreator,
    peerId: peerIdRef.current,

    // Actions
    createRoom,
    joinRoom,
    leaveRoom,
    destroyRoom,
    kickPeer,
    updateCursor,
    setCreatedAt: (createdAt: number, ttl: RoomTTL) => {
      const manager = roomManagerRef.current;
      if (!manager) return;
      manager.setCreatedAt(createdAt, ttl);
      setRemainingTime(manager.getRemainingTime());
    },

    // Computed
    formatRemainingTime,
    getRemainingPercent,
    isNearExpiry,
    isUrgentExpiry,
    getOnlinePeers,
  };
}
