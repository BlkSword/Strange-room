/**
 * 房间头部组件 - 商业风格
 */

'use client';

import { RoomTTL } from '@/types/room';
import { Clock, Users, Share2, Copy, Check, Crown, Shield, Trash2, Lock } from 'lucide-react';
import { useState } from 'react';
import { Button, Badge, Tooltip, Modal } from 'antd';

interface RoomHeaderProps {
  roomId: string;
  roomName: string;
  ttl: RoomTTL;
  remainingTime: number;
  onlineCount: number;
  isCreator: boolean;
  inviteLink?: string;
  encryptionEnabled?: boolean;
  encryptionKeyString?: string;
  onGenerateInvite?: () => void;
  onDestroyRoom?: () => void;
  onCopyEncryptionKey?: () => void;
}

export function RoomHeader({
  roomId,
  roomName,
  ttl,
  remainingTime,
  onlineCount,
  isCreator,
  inviteLink,
  encryptionEnabled = false,
  encryptionKeyString,
  onGenerateInvite,
  onDestroyRoom,
  onCopyEncryptionKey,
}: RoomHeaderProps) {
  const [copied, setCopied] = useState(false);
  const [destroyModalOpen, setDestroyModalOpen] = useState(false);

  // Debug: 渲染时的 isCreator 状态
  console.log('[RoomHeader] 渲染, isCreator:', isCreator, 'roomId:', roomId);

  // 销毁房间确认
  const handleDestroyRoom = () => {
    console.log('[RoomHeader] handleDestroyRoom 被调用, isCreator:', isCreator);
    setDestroyModalOpen(true);
  };

  const handleDestroyConfirm = () => {
    console.log('[RoomHeader] 用户确认销毁房间, 调用 onDestroyRoom');
    setDestroyModalOpen(false);
    onDestroyRoom?.();
  };

  // 格式化剩余时间
  const formatTime = (ms: number): string => {
    if (ms <= 0) return '已过期';

    const totalSeconds = Math.floor(ms / 1000);
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    if (hours > 0) {
      return `${hours}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
    }
    return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  };

  // 计算剩余百分比
  const getPercent = (): number => {
    const total = ttl * 60 * 60 * 1000;
    return Math.max(0, Math.min(100, (remainingTime / total) * 100));
  };

  // 判断是否即将过期
  const isNearExpiry = getPercent() < 10;
  const isUrgent = remainingTime < 5 * 60 * 1000;

  // 复制邀请链接
  const handleCopyInvite = async () => {
    if (!inviteLink) {
      onGenerateInvite?.();
      return;
    }

    try {
      await navigator.clipboard.writeText(inviteLink);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {}
  };

  return (
    <div className="bg-white border-b border-gray-200">
      <div className="px-6 py-4">
        <div className="flex items-center justify-between">
          {/* 左侧：房间信息 */}
          <div className="flex items-center gap-4">
            <div>
              <div className="flex items-center gap-3">
                <h1 className="text-xl font-bold text-gray-900">{roomName}</h1>
                {isCreator && (
                  <Tooltip title="房间创建者">
                    <Badge
                      count={<Crown size={12} className="text-amber-600" />}
                      showZero
                      className="bg-amber-50 border-amber-200"
                    />
                  </Tooltip>
                )}
              </div>
              <div className="flex items-center gap-4 mt-1 text-sm text-gray-500">
                <span className="flex items-center gap-1.5">
                  <span className="font-mono bg-gray-100 px-2 py-0.5 rounded text-xs">
                    #{roomId}
                  </span>
                </span>
                <span className="flex items-center gap-1.5">
                  <Users size={14} />
                  {onlineCount} 人在线
                </span>
                {encryptionEnabled && (
                  <Tooltip title="端到端加密已启用，消息内容只有房间成员可以查看">
                    <span className="flex items-center gap-1.5 text-green-600 bg-green-50 px-2 py-0.5 rounded-full">
                      <Lock size={12} />
                      加密
                    </span>
                  </Tooltip>
                )}
              </div>
            </div>
          </div>

          {/* 中间：倒计时 */}
          <div className="flex items-center gap-6">
            <div className="text-center">
              <div className={`flex items-center gap-2 text-2xl font-mono font-semibold ${
                isUrgent ? 'text-red-600' : isNearExpiry ? 'text-amber-600' : 'text-gray-900'
              }`}>
                <Clock size={20} className={isUrgent ? 'animate-pulse' : ''} />
                {formatTime(remainingTime)}
              </div>
              <div className="w-48 h-1.5 bg-gray-100 rounded-full mt-2 overflow-hidden">
                <div
                  className={`h-full transition-all duration-1000 ${
                    isUrgent ? 'bg-red-500' : isNearExpiry ? 'bg-amber-500' : 'bg-emerald-500'
                  }`}
                  style={{ width: `${getPercent()}%` }}
                />
              </div>
            </div>
          </div>

          {/* 右侧：邀请按钮和销毁按钮 */}
          <div className="flex items-center gap-3">
            {encryptionEnabled && isCreator && encryptionKeyString && (
              <Button
                onClick={() => {
                  onCopyEncryptionKey?.();
                  setCopied(true);
                  setTimeout(() => setCopied(false), 2000);
                }}
                icon={copied ? <Check size={16} /> : <Lock size={16} />}
                className="h-10 px-4 bg-white border-green-500 text-green-600 hover:bg-green-50 hover:border-green-600 rounded-lg font-medium shadow-sm"
              >
                {copied ? '已复制密钥' : '复制加密密钥'}
              </Button>
            )}
            {isCreator && (
              <Button
                onClick={(e) => {
                  console.log('[RoomHeader] 销毁按钮点击事件触发');
                  handleDestroyRoom();
                }}
                icon={<Trash2 size={16} />}
                className="h-10 px-5 bg-white border-red-300 text-red-600 hover:bg-red-50 hover:border-red-400 rounded-lg font-medium shadow-sm"
              >
                销毁房间
              </Button>
            )}
            <Button
              onClick={handleCopyInvite}
              icon={copied ? <Check size={16} /> : <Copy size={16} />}
              className="h-10 px-5 bg-blue-600 hover:bg-blue-700 border-none rounded-lg font-medium shadow-sm"
            >
              {copied ? '已复制' : '分享房间'}
            </Button>
          </div>
        </div>

        {/* 安全提示 */}
        {isCreator && (
          <div className="mt-4 flex items-center gap-2 px-4 py-2 bg-blue-50 rounded-lg">
            <Shield size={16} className="text-blue-600" />
            <span className="text-sm text-blue-700">
              作为创建者，你可以随时销毁此房间。所有数据将在房间关闭后永久删除。
            </span>
          </div>
        )}
      </div>

      {/* 销毁房间确认弹窗 */}
      <Modal
        title="确认销毁房间"
        open={destroyModalOpen}
        onOk={handleDestroyConfirm}
        onCancel={() => setDestroyModalOpen(false)}
        okText="确认销毁"
        cancelText="取消"
        okButtonProps={{ danger: true }}
        centered
        maskClosable
      >
        <p>此操作将永久销毁房间，所有数据将被删除且无法恢复。是否继续？</p>
      </Modal>
    </div>
  );
}
