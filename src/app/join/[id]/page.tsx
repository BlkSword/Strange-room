/**
 * 邀请链接页面 - 验证邀请并重定向到房间
 */

'use client';

import { useEffect, useState } from 'react';
import { useParams, useRouter, useSearchParams } from 'next/navigation';
import { validateToken, checkRoom } from '@/lib/server/api';
import { Modal, message, Input, Button } from 'antd';
import { Home } from 'lucide-react';
import { generateRandomNickname } from '@/lib/utils/hash';

export default function JoinPage() {
  const params = useParams();
  const router = useRouter();
  const searchParams = useSearchParams();
  const roomId = (params.id as string).toUpperCase();
  const urlToken = searchParams.get('token');

  const [isLoading, setIsLoading] = useState(true);
  const [isValid, setIsValid] = useState(false);
  const [isMounted, setIsMounted] = useState(false);
  const [nickname, setNickname] = useState('');
  const [modalVisible, setModalVisible] = useState(false);
  const [roomCheckDestroyed, setRoomCheckDestroyed] = useState(false);

  useEffect(() => {
    setIsMounted(true);
  }, []);

  useEffect(() => {
    // 仅在客户端运行
    if (typeof window === 'undefined' || !isMounted) return;

    // 检查是否有 token
    if (!urlToken) {
      message.error('邀请链接无效：缺少令牌');
      setIsLoading(false);
      return;
    }

    // 检查是否已经保存了 token（避免重复验证）
    const savedToken = localStorage.getItem(`room-token-${roomId}`);
    if (savedToken === urlToken && localStorage.getItem(`room-data-${roomId}`)) {
      // 已经验证过，直接跳转
      router.push(`/room/${roomId}?token=${urlToken}`);
      return;
    }

    // 验证令牌
    validateTokenAndShowModal();
  }, [roomId, urlToken, isMounted]);

  const validateTokenAndShowModal = async () => {
    if (!urlToken) return;

    try {
      // 先检查房间状态
      const roomCheck = await checkRoom(roomId);

      if (!roomCheck.exists) {
        setRoomCheckDestroyed(roomCheck.destroyed || false);
        setIsLoading(false);
        return;
      }

      // 调用 API 验证令牌
      const result = await validateToken(urlToken);

      if (!result.valid || result.roomId !== roomId) {
        message.error(result.error || '邀请链接无效或已过期');
        setIsLoading(false);
        return;
      }

      setIsValid(true);
      setModalVisible(true);
    } catch (error) {
      console.error('[Join] 验证令牌失败:', error);
      message.error('验证失败，请检查网络连接');
    } finally {
      setIsLoading(false);
    }
  };

  const handleJoin = () => {
    // 禁止匿名用户：如果没有输入昵称，生成随机昵称
    const finalNickname = nickname.trim() || generateRandomNickname();

    // 保存令牌到 localStorage
    localStorage.setItem(`room-token-${roomId}`, urlToken!);
    localStorage.setItem(`user-nickname-${roomId}`, finalNickname);

    // 创建基础房间数据（会在房间页面中完善）
    const roomData = {
      id: roomId,
      name: `房间 ${roomId}`,
      ttl: 24,
      createdAt: Date.now(),
      expiresAt: Date.now() + 24 * 60 * 60 * 1000,
      creatorPeerId: '',
      peers: {},
      destroyed: false,
    };
    localStorage.setItem(`room-data-${roomId}`, JSON.stringify(roomData));

    message.success('正在加入房间...');
    setModalVisible(false);

    // 跳转到房间页面
    setTimeout(() => {
      router.push(`/room/${roomId}?token=${urlToken}`);
    }, 500);
  };

  const handleCancel = () => {
    setModalVisible(false);
    router.push('/');
  };

  return (
    <>
      <div className="min-h-screen bg-sketch-background flex items-center justify-center px-6">
        <div className="text-center max-w-lg">
          {isLoading ? (
            <>
              <div className="w-16 h-16 border-4 border-sketch-gray border-t-sketch-black rounded-full animate-spin mx-auto mb-4" />
              <p className="text-sketch-gray font-medium font-cave text-lg">正在验证邀请链接...</p>
            </>
          ) : !isValid ? (
            <>
              <h1 className="text-5xl md:text-6xl font-marker mb-6 leading-tight hand-drawn-title text-sketch-black">
                无法加入房间
              </h1>
              <p className="text-xl text-sketch-gray mb-8 font-cave">
                {roomCheckDestroyed ? '房间已被销毁' : '房间不存在或已过期'}
              </p>
              <Button
                type="primary"
                size="large"
                icon={<Home size={20} />}
                className="hand-drawn-btn text-lg px-8"
                onClick={() => router.push('/')}
              >
                返回首页
              </Button>
            </>
          ) : (
            <>
              <div className="w-16 h-16 border-4 border-sketch-gray border-t-sketch-black rounded-full animate-spin mx-auto mb-4" />
              <p className="text-sketch-gray font-medium font-cave text-lg">正在进入房间...</p>
            </>
          )}
        </div>
      </div>

      <Modal
        title="加入房间"
        open={modalVisible && isMounted}
        onOk={handleJoin}
        onCancel={handleCancel}
        okText="加入"
        cancelText="取消"
        centered
      >
        <div className="py-4">
          <p className="mb-2 text-gray-600 font-medium">请输入您的昵称</p>
          <p className="mb-4 text-sm text-gray-500">不输入昵称将自动生成随机昵称</p>
          <Input
            placeholder="您的昵称"
            value={nickname}
            onChange={(e) => setNickname(e.target.value)}
            autoFocus
            maxLength={20}
            onPressEnter={handleJoin}
          />
          <p className="mt-4 text-sm text-gray-500">
            房间 ID: {roomId}
          </p>
        </div>
      </Modal>
    </>
  );
}
