/**
 * 房间头部组件 - 简约手绘风格
 */

'use client';

import { RoomTTL } from '@/types/room';
import { Clock, Users, Share2, Copy, Check, Crown, Shield, Trash2, Lock, Sparkles } from 'lucide-react';
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
  const [isCopying, setIsCopying] = useState(false);
  const [destroyModalOpen, setDestroyModalOpen] = useState(false);

  // 销毁房间确认
  const handleDestroyRoom = () => {
    setDestroyModalOpen(true);
  };

  const handleDestroyConfirm = () => {
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
    if (isCopying || copied) {
      return; // 防止多次点击
    }

    if (!inviteLink) {
      onGenerateInvite?.();
      return;
    }

    setIsCopying(true);

    try {
      await navigator.clipboard.writeText(inviteLink);
      setCopied(true);
      setTimeout(() => {
        setCopied(false);
        setIsCopying(false);
      }, 2000);
    } catch {
      setIsCopying(false);
    }
  };

  // 复制加密密钥
  const handleCopyEncryptionKey = () => {
    if (isCopying || copied) {
      return; // 防止多次点击
    }

    setIsCopying(true);
    onCopyEncryptionKey?.();
    setCopied(true);
    setTimeout(() => {
      setCopied(false);
      setIsCopying(false);
    }, 2000);
  };

  return (
    <div className="bg-sketch-card border-b-2 border-sketch-black">
      <div className="px-6 py-4">
        <div className="flex items-center justify-between">
          {/* 左侧：房间信息 */}
          <div className="flex items-center gap-4">
            <div>
              <div className="flex items-center gap-3">
                <h1 className="text-2xl font-bold text-sketch-black font-cave flex items-center gap-2">
                  <Sparkles size={20} />
                  {roomName}
                </h1>
                {isCreator && (
                  <Tooltip title="房间创建者">
                    <Badge
                      count={<Crown size={14} className="text-sketch-accent" />}
                      showZero
                      className="bg-sketch-light border-2 border-sketch-black"
                    />
                  </Tooltip>
                )}
              </div>
              <div className="flex items-center gap-4 mt-2 text-sm text-sketch-gray font-cave">
                <span className="flex items-center gap-1.5">
                  <span className="font-mono bg-sketch-light px-3 py-1 rounded-sketch text-xs text-sketch-black border-2 border-sketch-black">
                    #{roomId}
                  </span>
                </span>
                <span className="flex items-center gap-1.5">
                  <Users size={16} />
                  {onlineCount} 人在线
                </span>
                {encryptionEnabled && (
                  <Tooltip title="端到端加密已启用，消息内容只有房间成员可以查看">
                    <span className="flex items-center gap-1.5 text-sketch-black bg-sketch-light px-3 py-1 rounded-sketch border-2 border-sketch-black">
                      <Lock size={14} />
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
              <div className={`flex items-center gap-2 text-3xl font-mono font-semibold font-cave ${
                isUrgent ? 'text-red-500 animate-pulse' : isNearExpiry ? 'text-amber-600' : 'text-sketch-black'
              }`}>
                <Clock size={24} className={isUrgent ? 'animate-pulse' : ''} />
                {formatTime(remainingTime)}
              </div>
              <div className="w-52 h-2 bg-sketch-light rounded-sketch mt-2 overflow-hidden border-2 border-sketch-black">
                <div
                  className={`h-full transition-all duration-1000 rounded-sketch ${
                    isUrgent
                      ? 'bg-red-500'
                      : isNearExpiry
                      ? 'bg-amber-500'
                      : 'bg-sketch-accent'
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
                onClick={handleCopyEncryptionKey}
                disabled={isCopying}
                icon={copied ? <Check size={18} /> : <Lock size={18} />}
                className="hand-drawn-btn font-cave"
              >
                {copied ? '已复制密钥' : '复制加密密钥'}
              </Button>
            )}
            {isCreator && (
              <Button
                onClick={handleDestroyRoom}
                icon={<Trash2 size={18} />}
                className="hand-drawn-btn bg-red-600 hover:bg-red-700 font-cave"
                style={{ backgroundColor: '#dc2626', borderColor: '#dc2626' }}
              >
                销毁房间
              </Button>
            )}
            <Button
              onClick={handleCopyInvite}
              disabled={isCopying}
              icon={copied ? <Check size={18} /> : <Share2 size={18} />}
              className="hand-drawn-btn font-cave"
            >
              {copied ? '已复制' : '分享房间'}
            </Button>
          </div>
        </div>

        {/* 安全提示 */}
        {isCreator && (
          <div className="mt-4 flex items-center gap-2 px-4 py-3 bg-sketch-light rounded-sketch border-2 border-sketch-black">
            <Shield size={18} className="text-sketch-accent" />
            <span className="text-sm text-sketch-gray font-cave">
              作为创建者，你可以随时销毁此房间。所有数据将在房间关闭后永久删除。
            </span>
          </div>
        )}
      </div>

      {/* 销毁房间确认弹窗 */}
      <Modal
        title={
          <span className="text-sketch-black font-cave text-xl">确认销毁房间</span>
        }
        open={destroyModalOpen}
        onOk={handleDestroyConfirm}
        onCancel={() => setDestroyModalOpen(false)}
        okText="确认销毁"
        cancelText="取消"
        okButtonProps={{ danger: true }}
        centered
        maskClosable
      >
        <p className="text-sketch-gray font-cave">此操作将永久销毁房间，所有数据将被删除且无法恢复。是否继续？</p>
      </Modal>
    </div>
  );
}
