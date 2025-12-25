/**
 * 邀请链接页面 - 验证邀请并重定向到房间
 */

'use client';

import { useEffect, useState } from 'react';
import { useParams, useRouter, useSearchParams } from 'next/navigation';
import { validateToken } from '@/lib/server/api';
import { Modal, message, Input } from 'antd';

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
    const finalNickname = nickname.trim() || '匿名用户';

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
      <div className="h-screen flex items-center justify-center bg-white">
        <div className="text-center">
          {isLoading ? (
            <>
              <div className="w-12 h-12 border-4 border-blue-600 border-t-transparent rounded-full animate-spin mx-auto mb-4" />
              <p className="text-gray-600 font-medium">正在验证邀请链接...</p>
            </>
          ) : !isValid ? (
            <>
              <div className="w-16 h-16 bg-red-100 rounded-full flex items-center justify-center mx-auto mb-4">
                <svg className="w-8 h-8 text-red-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </div>
              <h2 className="text-xl font-bold text-gray-900 mb-2">邀请链接无效</h2>
              <p className="text-gray-600 mb-6">该链接可能已过期或不存在</p>
              <button
                onClick={() => router.push('/')}
                className="px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium"
              >
                返回首页
              </button>
            </>
          ) : (
            <>
              <div className="w-12 h-12 border-4 border-blue-600 border-t-transparent rounded-full animate-spin mx-auto mb-4" />
              <p className="text-gray-600 font-medium">正在进入房间...</p>
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
          <p className="mb-4 text-gray-600">请输入您的昵称</p>
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
