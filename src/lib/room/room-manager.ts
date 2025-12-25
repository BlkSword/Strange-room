/**
 * 房间管理器 - TTL 倒计时和房间生命周期
 */

import { nanoid } from 'nanoid';
import { Room, RoomTTL, PeerInfo } from '@/types/room';

export interface CreateRoomOptions {
  name?: string;
  ttl: RoomTTL;
  creatorNickname: string;
  creatorColor: string;
}

export class RoomManager {
  private room: Room | null = null;
  private countdownInterval: number | null = null;
  private destroyCallbacks: Set<(reason: 'expired' | 'creator_left' | 'manual') => void> = new Set();
  private tickCallbacks: Set<(remainingMs: number) => void> = new Set();

  /**
   * 创建新房间
   */
  createRoom(options: CreateRoomOptions): Room {
    const now = Date.now();
    const ttlMs = options.ttl * 60 * 60 * 1000;
    const roomId = this.generateRoomId();

    const creatorPeerId = nanoid();

    this.room = {
      id: roomId,
      name: options.name || `房间 ${roomId.slice(0, 6)}`,
      createdAt: now,
      ttl: options.ttl,
      expiresAt: now + ttlMs,
      creatorPeerId,
      peers: new Map([
        [
          creatorPeerId,
          {
            id: creatorPeerId,
            nickname: options.creatorNickname,
            color: options.creatorColor,
            joinedAt: now,
            isCreator: true,
            isOnline: true,
          },
        ],
      ]),
      destroyed: false,
    };

    // 启动倒计时
    this.startCountdown();

    console.log('[Room] 房间已创建:', this.room);
    return this.room;
  }

  /**
   * 加入房间
   */
  joinRoom(room: Room, peerInfo: Omit<PeerInfo, 'joinedAt' | 'isCreator'>): PeerInfo {
    const newPeer: PeerInfo = {
      ...peerInfo,
      joinedAt: Date.now(),
      isCreator: peerInfo.id === room.creatorPeerId,
    };

    room.peers.set(newPeer.id, newPeer);
    this.room = room;

    // 如果是第一个加入的人（非创建者），启动倒计时
    if (this.countdownInterval === null) {
      this.startCountdown();
    }

    console.log('[Room] 加入房间:', newPeer);
    return newPeer;
  }

  /**
   * 离开房间
   */
  leaveRoom(peerId: string) {
    if (!this.room) return;

    const peer = this.room.peers.get(peerId);
    if (peer) {
      peer.isOnline = false;
    }

    // 如果是创建者离开，立即销毁房间
    if (peer?.isCreator) {
      this.destroyRoom('creator_left');
    }

    console.log('[Room] 离开房间:', peerId);
  }

  /**
   * 销毁房间
   */
  destroyRoom(reason: 'expired' | 'creator_left' | 'manual' = 'expired') {
    if (!this.room || this.room.destroyed) return;

    console.log('[Room] 房间销毁:', reason);

    this.room.destroyed = true;

    // 停止倒计时
    this.stopCountdown();

    // 触发销毁回调，传递 reason
    this.destroyCallbacks.forEach(cb => cb(reason));
  }

  /**
   * 启动倒计时
   */
  private startCountdown() {
    if (this.countdownInterval !== null || !this.room) return;

    this.countdownInterval = window.setInterval(() => {
      const now = Date.now();
      const remaining = this.room!.expiresAt - now;

      // 触发 tick 回调
      this.tickCallbacks.forEach(cb => cb(remaining));

      // 检查是否过期
      if (remaining <= 0) {
        this.destroyRoom('expired');
      }
    }, 1000);
  }

  /**
   * 停止倒计时
   */
  private stopCountdown() {
    if (this.countdownInterval !== null) {
      clearInterval(this.countdownInterval);
      this.countdownInterval = null;
    }
  }

  /**
   * 获取剩余时间（毫秒）
   */
  getRemainingTime(): number {
    if (!this.room || this.room.destroyed) return 0;
    return Math.max(0, this.room.expiresAt - Date.now());
  }

  /**
   * 获取剩余时间（格式化）
   */
  getRemainingTimeFormatted(): string {
    const ms = this.getRemainingTime();
    if (ms <= 0) return '已过期';

    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) {
      return `${days}天 ${hours % 24}小时 ${minutes % 60}分钟`;
    } else if (hours > 0) {
      return `${hours}小时 ${minutes % 60}分钟`;
    } else if (minutes > 0) {
      return `${minutes}分钟 ${seconds % 60}秒`;
    } else {
      return `${seconds}秒`;
    }
  }

  /**
   * 获取剩余时间百分比
   */
  getRemainingTimePercent(): number {
    if (!this.room) return 0;
    const total = this.room.ttl * 60 * 60 * 1000;
    const remaining = this.getRemainingTime();
    return Math.max(0, Math.min(100, (remaining / total) * 100));
  }

  /**
   * 是否即将过期（剩余时间小于10%）
   */
  isNearExpiry(): boolean {
    return this.getRemainingTimePercent() < 10;
  }

  /**
   * 是否紧急过期（剩余时间小于5分钟）
   */
  isUrgentExpiry(): boolean {
    return this.getRemainingTime() < 5 * 60 * 1000;
  }

  /**
   * 生成房间 ID (6位随机字符)
   */
  private generateRoomId(): string {
    const chars = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789'; // 去除易混淆字符
    let result = '';
    for (let i = 0; i < 6; i++) {
      result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    return result;
  }

  /**
   * 获取房间信息
   */
  getRoom(): Room | null {
    return this.room;
  }

  /**
   * 获取在线用户列表
   */
  getOnlinePeers(): PeerInfo[] {
    if (!this.room) return [];
    return Array.from(this.room.peers.values()).filter(p => p.isOnline);
  }

  /**
   * 更新用户光标位置
   */
  updatePeerCursor(peerId: string, cursor: { x: number; y: number; page: 'chat' | 'whiteboard' | 'editor' }) {
    if (!this.room) return;
    const peer = this.room.peers.get(peerId);
    if (peer) {
      peer.cursor = cursor;
    }
  }

  /**
   * 踢出用户（仅创建者）
   */
  kickPeer(creatorPeerId: string, targetPeerId: string): boolean {
    if (!this.room || this.room.creatorPeerId !== creatorPeerId) {
      return false;
    }

    this.room.peers.delete(targetPeerId);
    return true;
  }

  /**
   * 注册销毁回调
   */
  onDestroy(callback: (reason: 'expired' | 'creator_left' | 'manual') => void) {
    this.destroyCallbacks.add(callback);
  }

  /**
   * 移除销毁回调
   */
  offDestroy(callback: (reason: 'expired' | 'creator_left' | 'manual') => void) {
    this.destroyCallbacks.delete(callback);
  }

  /**
   * 注册倒计时回调
   */
  onTick(callback: (remainingMs: number) => void) {
    this.tickCallbacks.add(callback);
  }

  /**
   * 移除倒计时回调
   */
  offTick(callback: (remainingMs: number) => void) {
    this.tickCallbacks.delete(callback);
  }

  /**
   * 清理资源
   */
  dispose() {
    this.stopCountdown();
    this.destroyCallbacks.clear();
    this.tickCallbacks.clear();
  }
}
