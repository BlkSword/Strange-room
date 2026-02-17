/**
 * 房间头部组件 - 简约手绘风格
 */

'use client';

import { RoomTTL } from '@/types/room';
import { Clock, Users, Share2, Check, Crown, Shield, Trash2, Lock, Sparkles, Download } from 'lucide-react';
import { useState } from 'react';
import { Button, Tooltip, Modal, Input } from 'antd';

interface OnlineUser {
  id: string;
  nickname: string;
  color: string;
  isCreator?: boolean;
}

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
  onlineUsers?: OnlineUser[];
  onGenerateInvite?: () => void;
  onDestroyRoom?: () => void;
  onCopyEncryptionKey?: () => void;
  onExportRoom?: (password?: string) => Promise<boolean>;
  isExporting?: boolean;
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
  onlineUsers = [],
  onGenerateInvite,
  onDestroyRoom,
  onCopyEncryptionKey,
  onExportRoom,
  isExporting = false,
}: RoomHeaderProps) {
  const [copied, setCopied] = useState(false);
  const [isCopying, setIsCopying] = useState(false);
  const [destroyModalOpen, setDestroyModalOpen] = useState(false);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportPassword, setExportPassword] = useState('');

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

  // 打开导出弹窗
  const handleExportClick = () => {
    setExportPassword('');
    setExportModalOpen(true);
  };

  // 确认导出
  const handleExportConfirm = async () => {
    const password = exportPassword.trim() || undefined;
    const success = await onExportRoom?.(password);
    if (success) {
      setExportModalOpen(false);
      setExportPassword('');
    }
  };

  // 取消导出
  const handleExportCancel = () => {
    setExportModalOpen(false);
    setExportPassword('');
  };

  return (
    <div className="bg-sketch-card border-b-2 border-sketch-black">
      <div className="px-6 py-4">
        <div className="flex items-center justify-between">
          {/* 左侧：房间信息 */}
          <div className="flex items-center gap-4 w-1/3">
            <div>
              <div className="flex items-center gap-3">
                <h1 className="text-2xl font-bold text-sketch-black font-cave flex items-center gap-2">
                  <Sparkles size={20} />
                  {roomName}
                </h1>
                {isCreator && (
                  <Tooltip title="房间创建者">
                    <Crown size={18} className="text-sketch-accent" />
                  </Tooltip>
                )}
              </div>
              <div className="flex items-center gap-4 mt-2 text-sm text-sketch-gray font-cave">
                <span className="flex items-center gap-1.5">
                  <span className="font-mono bg-sketch-light px-3 py-1 rounded-sketch text-xs text-sketch-black border-2 border-sketch-black">
                    #{roomId}
                  </span>
                </span>
                {/* 在线用户头像 */}
                <Tooltip
                  title={
                    <div className="max-w-xs">
                      <div className="font-semibold mb-2 text-sm">在线用户 ({onlineUsers.length})</div>
                      <div className="space-y-1">
                        {onlineUsers.map((user) => (
                          <div key={user.id} className="flex items-center gap-2 text-xs">
                            <div
                              className="w-5 h-5 rounded-full flex items-center justify-center text-white text-xs font-medium"
                              style={{ backgroundColor: user.color }}
                            >
                              {user.nickname.charAt(0).toUpperCase()}
                            </div>
                            <span className="flex-1">{user.nickname}</span>
                            {user.isCreator && <Crown size={10} className="text-sketch-accent" />}
                          </div>
                        ))}
                      </div>
                    </div>
                  }
                  color="white"
                  overlayClassName="user-tooltip"
                >
                  <span className="flex items-center gap-1.5 cursor-help hover:text-sketch-black transition-colors">
                    <Users size={16} />
                    {onlineCount} 人在线
                    {/* 显示最多3个头像 */}
                    {onlineUsers.length > 0 && (
                      <span className="flex -space-x-2 ml-1">
                        {onlineUsers.slice(0, 3).map((user) => (
                          <div
                            key={user.id}
                            className="w-6 h-6 rounded-full border-2 border-white flex items-center justify-center text-white text-xs font-medium"
                            style={{ backgroundColor: user.color }}
                            title={user.nickname}
                          >
                            {user.nickname.charAt(0).toUpperCase()}
                          </div>
                        ))}
                        {onlineUsers.length > 3 && (
                          <div className="w-6 h-6 rounded-full border-2 border-white bg-sketch-gray flex items-center justify-center text-white text-xs font-medium">
                            +{onlineUsers.length - 3}
                          </div>
                        )}
                      </span>
                    )}
                  </span>
                </Tooltip>
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
          <div className="flex items-center justify-center flex-1">
            <div className={`flex items-center gap-2 text-3xl font-mono font-semibold font-cave ${
              isUrgent ? 'text-red-500 animate-pulse' : isNearExpiry ? 'text-amber-600' : 'text-sketch-black'
            }`}>
              <Clock size={24} className={isUrgent ? 'animate-pulse' : ''} />
              {formatTime(remainingTime)}
            </div>
          </div>

          {/* 右侧：邀请按钮和销毁按钮 */}
          <div className="flex items-center gap-3 w-1/3 justify-end">
            {/* 导出按钮 */}
            <Button
              onClick={handleExportClick}
              disabled={isExporting}
              icon={<Download size={18} />}
              className="hand-drawn-btn font-cave"
            >
              {isExporting ? '导出中...' : '导出'}
            </Button>

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

      {/* 导出房间数据弹窗 */}
      <Modal
        title={
          <span className="text-sketch-black font-cave text-xl">导出房间数据</span>
        }
        open={exportModalOpen}
        onOk={handleExportConfirm}
        onCancel={handleExportCancel}
        okText="导出"
        cancelText="取消"
        confirmLoading={isExporting}
        centered
        maskClosable
      >
        <div className="py-2">
          <p className="text-sketch-gray font-cave mb-4">
            将导出房间的所有数据，包括聊天记录、代码、白板内容等。
          </p>
          <div className="mb-4">
            <label className="block text-sm font-medium text-sketch-gray font-cave mb-2">
              设置密码（可选）
            </label>
            <Input.Password
              placeholder="留空则不加密"
              value={exportPassword}
              onChange={(e) => setExportPassword(e.target.value)}
              className="font-cave"
            />
            <p className="text-xs text-gray-500 mt-1 font-cave">
              设置密码后，导出的文件将被加密，需要密码才能导入
            </p>
          </div>
        </div>
      </Modal>
    </div>
  );
}
