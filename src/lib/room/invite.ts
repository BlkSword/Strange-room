/**
 * 一次性邀请链接生成器 - 使用自编码 Token（无服务器存储）
 */

import { nanoid } from 'nanoid';

export interface InviteLinkOptions {
  roomId: string;
  maxUses?: number;
  expireIn?: number; // 毫秒，默认24小时
}

export interface InviteData {
  roomId: string;
  createdAt: number;
  expiresAt: number;
  maxUses: number;
  // 注意：usedCount 和 usedBy 无法在纯客户端跨设备同步
  // 所以我们移除这些字段，依赖 expiresAt 和 maxUses 的概念性限制
}

/**
 * 入场券 - 证明用户有权访问某个房间
 */
interface AccessTicket {
  roomId: string;
  inviteToken: string;
  grantedAt: number;
  expiresAt: number;
}

export class InviteManager {
  private static readonly TICKET_KEY_PREFIX = 'room-access-ticket-';
  private static readonly TICKET_VALIDITY = 30 * 60 * 1000; // 入场券30分钟有效期

  /**
   * 将邀请数据编码为 token (Base64)
   */
  private encodeToken(data: InviteData): string {
    const json = JSON.stringify(data);
    return btoa(encodeURIComponent(json));
  }

  /**
   * 从 token 解码邀请数据
   */
  private decodeToken(token: string): InviteData | null {
    try {
      const json = decodeURIComponent(atob(token));
      return JSON.parse(json) as InviteData;
    } catch {
      return null;
    }
  }

  /**
   * 生成入场券并保存到 localStorage
   */
  grantAccessTicket(roomId: string, inviteToken: string, roomExpiresAt: number): void {
    const ticket: AccessTicket = {
      roomId,
      inviteToken,
      grantedAt: Date.now(),
      expiresAt: Math.min(Date.now() + InviteManager.TICKET_VALIDITY, roomExpiresAt),
    };
    localStorage.setItem(
      `${InviteManager.TICKET_KEY_PREFIX}${roomId}`,
      JSON.stringify(ticket)
    );
  }

  /**
   * 验证入场券
   */
  validateAccessTicket(roomId: string): {
    hasTicket: boolean;
    isValid: boolean;
    reason?: string;
  } {
    try {
      const ticketKey = `${InviteManager.TICKET_KEY_PREFIX}${roomId}`;
      const ticketData = localStorage.getItem(ticketKey);

      if (!ticketData) {
        return { hasTicket: false, isValid: false, reason: '没有入场券' };
      }

      const ticket: AccessTicket = JSON.parse(ticketData);

      // 检查房间ID是否匹配
      if (ticket.roomId !== roomId) {
        return { hasTicket: true, isValid: false, reason: '入场券房间不匹配' };
      }

      // 检查是否过期
      if (Date.now() > ticket.expiresAt) {
        localStorage.removeItem(ticketKey);
        return { hasTicket: true, isValid: false, reason: '入场券已过期' };
      }

      // 验证原始邀请token是否还有效
      const inviteValidation = this.validateInviteLink(roomId, ticket.inviteToken);
      if (!inviteValidation.valid) {
        localStorage.removeItem(ticketKey);
        return { hasTicket: true, isValid: false, reason: inviteValidation.reason };
      }

      return { hasTicket: true, isValid: true };
    } catch (e) {
      return { hasTicket: true, isValid: false, reason: '入场券验证失败' };
    }
  }

  /**
   * 检查是否是房间创建者
   */
  isRoomCreator(roomId: string): boolean {
    try {
      const roomData = localStorage.getItem(`room-data-${roomId}`);
      if (!roomData) return false;

      const room = JSON.parse(roomData);
      // 如果有 creatorPeerId 和 peers 信息，可以验证
      // 简化版本：检查 localStorage 中是否有创建标记
      return localStorage.getItem(`room-creator-${roomId}`) === 'true';
    } catch {
      return false;
    }
  }

  /**
   * 标记为房间创建者
   */
  markAsRoomCreator(roomId: string): void {
    localStorage.setItem(`room-creator-${roomId}`, 'true');
  }

  /**
   * 生成一次性邀请链接
   */
  generateInviteLink(options: InviteLinkOptions): string {
    const now = Date.now();
    const expireIn = options.expireIn || 24 * 60 * 60 * 1000; // 默认24小时

    const inviteData: InviteData = {
      roomId: options.roomId,
      createdAt: now,
      expiresAt: now + expireIn,
      maxUses: options.maxUses || 10, // 默认最多10人使用
    };

    const token = this.encodeToken(inviteData);

    // 生成链接
    const baseUrl = typeof window !== 'undefined' ? window.location.origin : '';
    return `${baseUrl}/join/${options.roomId}?token=${token}`;
  }

  /**
   * 验证邀请链接
   */
  validateInviteLink(roomId: string, token: string): {
    valid: boolean;
    reason?: string;
    inviteData?: InviteData;
  } {
    const inviteData = this.decodeToken(token);

    // Token 无法解码
    if (!inviteData) {
      return { valid: false, reason: '邀请链接无效' };
    }

    // 房间ID不匹配
    if (inviteData.roomId !== roomId) {
      return { valid: false, reason: '房间ID不匹配' };
    }

    // Token 已过期
    if (Date.now() > inviteData.expiresAt) {
      return { valid: false, reason: '邀请链接已过期' };
    }

    return { valid: true, inviteData };
  }

  /**
   * 使用邀请链接（标记为已使用）
   * 注意：由于是纯 P2P 应用，我们只能在本地标记使用状态
   */
  useInviteLink(token: string, peerId: string): boolean {
    // 存储已使用的 token 到 localStorage
    try {
      const usedKey = `used-invite-${token}`;
      const usedData = localStorage.getItem(usedKey);

      if (usedData) {
        const parsed = JSON.parse(usedData);
        // 检查是否已经达到最大使用次数
        if (parsed.usedCount >= parsed.maxUses) {
          return false;
        }
        parsed.usedCount++;
        if (!parsed.usedBy) parsed.usedBy = [];
        parsed.usedBy.push(peerId);
        localStorage.setItem(usedKey, JSON.stringify(parsed));
      } else {
        // 首次使用
        const inviteData = this.decodeToken(token);
        if (inviteData) {
          localStorage.setItem(usedKey, JSON.stringify({
            usedCount: 1,
            maxUses: inviteData.maxUses,
            usedBy: [peerId],
            expiresAt: inviteData.expiresAt,
          }));
        }
      }
      return true;
    } catch {
      return false;
    }
  }

  /**
   * 检查本地是否已使用过此邀请链接
   */
  hasUsedInvite(token: string): boolean {
    try {
      const usedKey = `used-invite-${token}`;
      return localStorage.getItem(usedKey) !== null;
    } catch {
      return false;
    }
  }

  /**
   * 获取邀请链接状态
   */
  getInviteStatus(token: string): {
    exists: boolean;
    remainingUses?: number;
    expiresAt?: number;
    isExpired: boolean;
  } {
    const inviteData = this.decodeToken(token);

    if (!inviteData) {
      return { exists: false, isExpired: false };
    }

    const isExpired = Date.now() > inviteData.expiresAt;

    // 检查本地使用次数
    let usedCount = 0;
    try {
      const usedKey = `used-invite-${token}`;
      const usedData = localStorage.getItem(usedKey);
      if (usedData) {
        usedCount = JSON.parse(usedData).usedCount || 0;
      }
    } catch {}

    const remainingUses = Math.max(0, inviteData.maxUses - usedCount);

    return {
      exists: true,
      remainingUses,
      expiresAt: inviteData.expiresAt,
      isExpired,
    };
  }

  /**
   * 生成房间二维码数据（用于扫描加入）
   */
  generateQRCodeData(roomId: string, token: string): string {
    return JSON.stringify({
      type: 'strange-room-invite',
      roomId,
      token,
      version: '1.0',
    });
  }

  /**
   * 解析二维码数据
   */
  parseQRCodeData(data: string): {
    valid: boolean;
    roomId?: string;
    token?: string;
  } {
    try {
      const parsed = JSON.parse(data);

      if (parsed.type === 'strange-room-invite' && parsed.roomId && parsed.token) {
        return {
          valid: true,
          roomId: parsed.roomId,
          token: parsed.token,
        };
      }

      return { valid: false };
    } catch {
      return { valid: false };
    }
  }
}

// 全局单例
export const inviteManager = new InviteManager();
