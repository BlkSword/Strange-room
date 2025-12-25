/**
 * 房间页面 - 动态路由
 * 集成聊天、白板、代码编辑器
 */

'use client';

import { useEffect, useState, useCallback, useRef } from 'react';
import { useParams, useRouter, useSearchParams } from 'next/navigation';
import { RoomHeader } from '@/components/Room/RoomHeader';
import { RoomLayout } from '@/components/Room/RoomLayout';
import { UserList } from '@/components/Room/UserList';
import { ChatBox } from '@/components/Chat/ChatBox';
import { Canvas } from '@/components/Whiteboard/Canvas';
import { MonacoEditor } from '@/components/Editor/MonacoEditor';
import { useRoom } from '@/hooks/useRoom';
import { useYjs } from '@/hooks/useYjs';
import { generateToken } from '@/lib/server/api';
import { Room } from '@/types/room';
import { message, Modal } from 'antd';
import { generateStableIdFromNickname } from '@/lib/utils/hash';

const USER_COLORS = [
  '#FF6B6B', '#4ECDC4', '#45B7D1', '#FFA07A', '#98D8C8',
  '#F7DC6F', '#BB8FCE', '#85C1E2', '#F8B739', '#52C785',
];

export default function RoomPage() {
  const params = useParams();
  const router = useRouter();
  const searchParams = useSearchParams();
  const roomId = (params.id as string).toUpperCase();

  // 从 URL 或 localStorage 获取 token
  const urlToken = searchParams.get('token');
  const [token, setToken] = useState<string>(() => {
    if (urlToken) return urlToken;
    if (typeof window !== 'undefined') {
      return localStorage.getItem(`room-token-${roomId}`) || '';
    }
    return '';
  });

  // 从 URL 获取加密密钥参数
  const urlEncryptionKey = searchParams.get('key');

  const [nickname, setNickname] = useState('');
  const [nicknameInput, setNicknameInput] = useState('');
  const [inviteLink, setInviteLink] = useState<string>('');
  const [roomInfo, setRoomInfo] = useState<Room | null>(null);
  const [userColor] = useState<string>(() =>
    USER_COLORS[Math.floor(Math.random() * USER_COLORS.length)]
  );
  const [currentCode, setCurrentCode] = useState<string>('// 在这里开始编写代码...\n');

  // 获取或生成用户的 peer ID（基于昵称）
  // 初始为临时 ID，将在昵称设置后更新
  const [userPeerId, setUserPeerId] = useState<string>('temp_user');

  // 当昵称变化时，更新 userPeerId
  useEffect(() => {
    if (nickname && nickname.trim()) {
      const newPeerId = generateStableIdFromNickname(nickname, roomId);
      if (newPeerId !== userPeerId) {
        console.log('[Room] 昵称变化，更新 userPeerId:', nickname, '->', newPeerId);
        setUserPeerId(newPeerId);
      }
    }
  }, [nickname, roomId]);

  // 状态
  const [isLoading, setIsLoading] = useState(true);
  const [accessDenied, setAccessDenied] = useState(false);
  const [hasToken, setHasToken] = useState(!!token);
  const [destroyModalOpen, setDestroyModalOpen] = useState(false);
  const [destroyReason, setDestroyReason] = useState<'expired' | 'creator_left' | 'manual'>('expired');
  const [nicknameModalOpen, setNicknameModalOpen] = useState(false);
  const [pendingRoomData, setPendingRoomData] = useState<{ room: Room | null; isCreator: boolean }>({ room: null, isCreator: false });

  // 跟踪时间是否已校正，避免重复校正
  const timeSyncedRef = useRef(false);

  // 房间销毁处理回调
  const handleRoomDestroyed = useCallback((reason: 'expired' | 'creator_left' | 'manual') => {
    console.log('[Room] handleRoomDestroyed 被调用, reason:', reason);
    setDestroyReason(reason);
    setDestroyModalOpen(true);
  }, []);

  // Hooks
  const { room, remainingTime, isCreator, peerId, joinRoom, destroyRoom, formatRemainingTime, setCreatedAt } = useRoom({
    peerId: userPeerId,
    onRoomDestroyed: handleRoomDestroyed,
  });

  // Debug: 监听 isCreator 变化
  useEffect(() => {
    console.log('[Room] isCreator 状态变化:', isCreator);
  }, [isCreator]);

  // 获取或初始化加密密钥
  const [encryptionKey, setEncryptionKey] = useState<string>(() => {
    if (typeof window !== 'undefined') {
      return localStorage.getItem(`room-encryption-key-${roomId}`) || '';
    }
    return '';
  });
  const [encryptionEnabledState, setEncryptionEnabledState] = useState<boolean>(() => {
    if (typeof window !== 'undefined') {
      return localStorage.getItem(`room-encryption-enabled-${roomId}`) === 'true';
    }
    return false;
  });

  const { isConnected, onlineUsers, chatMessages, decryptedMessages, sendMessage, updateCode, getCode, yjs, updateUserInfo, onCodeChange, peerId: yjsPeerId, initEncryption, importEncryptionKey, hasEncryption } = useYjs({
    roomId,
    peerId: userPeerId, // 传递基于昵称生成的 peerId
    userName: nickname,
    userColor,
    token, // 传递 token 给 Yjs
    encryptionKey, // 传递加密密钥
    encryptionEnabled: !!encryptionKey, // 启用加密
  });

  // 初始化房间
  useEffect(() => {
    if (typeof window === 'undefined') return;

    // 检查是否有 token
    const savedToken = localStorage.getItem(`room-token-${roomId}`);
    const finalToken = urlToken || savedToken;

    if (!finalToken || finalToken.trim() === '') {
      console.error('[Room] Token 为空或无效，拒绝访问');
      setAccessDenied(true);
      setIsLoading(false);
      return;
    }

    setToken(finalToken);
    setHasToken(true);

    // 加载保存的房间数据
    const savedNickname = localStorage.getItem(`user-nickname-${roomId}`);
    const savedRoomData = localStorage.getItem(`room-data-${roomId}`);

    // 如果URL中包含加密密钥，保存到localStorage
    if (urlEncryptionKey) {
      console.log('[Room] 从URL获取到加密密钥');
      localStorage.setItem(`room-encryption-key-${roomId}`, urlEncryptionKey);
      setEncryptionKey(urlEncryptionKey);
      setEncryptionEnabledState(true);
    }

    if (savedRoomData) {
      try {
        const parsed = JSON.parse(savedRoomData);
        setRoomInfo(parsed);

        if (savedNickname) {
          setNickname(savedNickname);

          // 检查是否是创建者 - 比对基于昵称生成的 peer ID
          const amICreator = parsed.creatorPeerId === userPeerId;

          console.log('[Room] 返回用户 - 我的peerId:', userPeerId, '创建者peerId:', parsed.creatorPeerId, '是创建者:', amICreator);

          const room: Room = {
            ...parsed,
            peers: new Map(),
            destroyed: false,
          };

          // 加入房间，传入是否是创建者
          joinRoom(room, savedNickname, amICreator);
        } else {
          // 首次进入，需要先输入昵称，然后检查是否是创建者
          // 先让用户输入昵称，输入后再检查是否是创建者
          const amICreator = false; // 先设为 false，等输入昵称后再重新计算
          console.log('[Room] 首次进入 - 需要输入昵称');
          showNicknameModal(parsed, amICreator);
        }
      } catch (e) {
        console.error('Failed to parse room info:', e);
        setAccessDenied(true);
      }
    } else {
      // 首次访问（通过邀请链接）
      showNicknameModal(null);
    }

    setIsLoading(false);
  }, [roomId, urlToken]);

  // 监听 Yjs 代码变化
  useEffect(() => {
    if (!onCodeChange) return;
    const unsubscribe = onCodeChange((newCode) => {
      setCurrentCode(newCode);
    });
    return () => {
      // 正确清理监听器
      if (unsubscribe) unsubscribe();
    };
  }, [onCodeChange]);

  // 初始加载代码
  useEffect(() => {
    if (yjs) {
      setCurrentCode(getCode());
    }
  }, [yjs]);

  // 同步 Yjs 中的房间信息
  useEffect(() => {
    if (yjs && room) {
      yjs.setRoomInfo('roomData', JSON.stringify(room));
    }
  }, [yjs, room]);

  // 从 Yjs 同步房间创建时间，校正倒计时（仅连接时执行一次）
  useEffect(() => {
    if (!yjs || !room) return;

    // 只在首次且有 roomData 时同步一次
    if (timeSyncedRef.current) return;
    timeSyncedRef.current = true;

    const roomDataStr = yjs.getRoomInfo('roomData');
    if (roomDataStr) {
      try {
        const roomData = JSON.parse(roomDataStr);
        if (roomData.createdAt && roomData.ttl) {
          console.log('[Room] 从 Yjs 同步房间创建时间:', new Date(roomData.createdAt).toISOString());
          setCreatedAt(roomData.createdAt, roomData.ttl);
        }
      } catch (e) {
        console.error('[Room] 解析 Yjs roomData 失败:', e);
      }
    }
  }, [yjs, room, setCreatedAt]);

  // 当 nickname 改变时，更新 Yjs 用户信息
  useEffect(() => {
    if (nickname && updateUserInfo) {
      updateUserInfo(nickname, userColor);
    }
  }, [nickname, userColor, updateUserInfo]);

  // 初始化加密
  useEffect(() => {
    const initializeEncryption = async () => {
      // 检查是否启用了加密且 yjs 已准备好
      if (!encryptionEnabledState || !yjs || !hasEncryption || !nickname) {
        return;
      }

      // 如果已有保存的密钥，直接导入
      if (encryptionKey) {
        console.log('[Room] 导入已有加密密钥');
        await importEncryptionKey(encryptionKey);
        return;
      }

      // 如果是创建者且没有密钥，生成新密钥
      if (isCreator && room) {
        console.log('[Room] 创建者生成新的加密密钥');
        const newKeyString = await initEncryption();
        if (newKeyString) {
          setEncryptionKey(newKeyString);
          localStorage.setItem(`room-encryption-key-${roomId}`, newKeyString);
          message.success('端到端加密已启用');
        }
      } else {
        console.log('[Room] 等待获取加密密钥...');
      }
    };

    initializeEncryption();
  }, [encryptionEnabledState, yjs, hasEncryption, nickname, isCreator, room]);

  // 显示昵称输入弹窗
  const showNicknameModal = useCallback((existingRoom: any, isCreator: boolean = false) => {
    console.log('[Room] showNicknameModal 被调用, isCreator:', isCreator);
    setPendingRoomData({ room: existingRoom, isCreator });
    setNicknameInput('');
    setNicknameModalOpen(true);
  }, []);

  // 处理昵称输入确认
  const handleNicknameConfirm = useCallback(() => {
    const name = nicknameInput.trim() || '匿名用户';
    setNickname(name);
    setNicknameModalOpen(false);

    // 基于昵称生成并更新 userPeerId
    const newPeerId = generateStableIdFromNickname(name, roomId);
    setUserPeerId(newPeerId);
    console.log('[Room] 昵称设置:', name, '生成 userPeerId:', newPeerId);

    if (pendingRoomData.room) {
      // 重新计算是否是创建者
      const amICreator = pendingRoomData.room.creatorPeerId === newPeerId;
      const room: Room = {
        ...pendingRoomData.room,
        peers: new Map(),
        destroyed: false,
      };
      console.log('[Room] 昵称确认, 调用 joinRoom, isCreator:', amICreator);
      joinRoom(room, name, amICreator);
    }

    localStorage.setItem(`user-nickname-${roomId}`, name);
  }, [nicknameInput, roomId, pendingRoomData, joinRoom]);

  // 处理昵称输入取消
  const handleNicknameCancel = useCallback(() => {
    setNicknameModalOpen(false);
    router.push('/');
  }, [router]);

  // 获取销毁原因对应的文本
  const getDestroyReasonText = useCallback((reason: 'expired' | 'creator_left' | 'manual') => {
    switch (reason) {
      case 'expired':
        return { title: '房间已销毁', content: '房间有效期已结束，所有数据已清除。' };
      case 'creator_left':
        return { title: '房间已销毁', content: '创建者已离开，房间已关闭。' };
      case 'manual':
        return { title: '房间已销毁', content: '房间已被手动销毁。' };
    }
  }, []);

  // 销毁通知 Modal 确认处理
  const handleDestroyConfirm = useCallback(() => {
    // 清理本地数据
    localStorage.removeItem(`room-data-${roomId}`);
    localStorage.removeItem(`user-nickname-${roomId}`);
    localStorage.removeItem(`room-token-${roomId}`);
    yjs?.clearAllData();
    setDestroyModalOpen(false);
    router.push('/');
  }, [roomId, yjs, router]);

  // 销毁房间处理
  const handleDestroyRoom = useCallback(() => {
    console.log('[Room] 点击销毁房间按钮, isCreator:', isCreator, 'peerId:', peerId);

    if (!isCreator) {
      message.error('只有房间创建者才能销毁房间');
      return;
    }

    console.log('[Room] 调用 destroyRoom...');
    // 调用 destroyRoom
    const success = destroyRoom();

    console.log('[Room] destroyRoom 返回:', success);
    if (success) {
      message.success('房间已销毁');
    } else {
      message.error('销毁房间失败');
    }
  }, [isCreator, destroyRoom, peerId]);

  // 生成邀请链接
  const handleGenerateInvite = useCallback(async () => {
    if (!room) return;

    // 调用 API 生成新令牌
    const result = await generateToken(room.id);

    if (!result.success || !result.token) {
      message.error('生成邀请链接失败');
      return;
    }

    const newToken = result.token;
    const link = `${window.location.origin}/join/${room.id}?token=${newToken}`;

    setInviteLink(link);

    // 自动复制（处理不支持 clipboard API 的情况）
    if (navigator.clipboard) {
      navigator.clipboard.writeText(link).then(() => {
        message.success('邀请链接已复制到剪贴板');
      }).catch(() => {
        message.success('邀请链接已生成（请手动复制）');
      });
    } else {
      // 降级方案：使用传统的 textarea 方法
      const textarea = document.createElement('textarea');
      textarea.value = link;
      textarea.style.position = 'fixed';
      textarea.style.opacity = '0';
      document.body.appendChild(textarea);
      textarea.select();
      try {
        document.execCommand('copy');
        message.success('邀请链接已复制到剪贴板');
      } catch {
        message.success('邀请链接已生成（请手动复制）');
      }
      document.body.removeChild(textarea);
    }
  }, [room]);

  // 发送消息
  const handleSendMessage = useCallback((content: string, type: 'text' | 'image') => {
    sendMessage(content, type);
  }, [sendMessage]);

  // 代码变化
  const handleCodeChange = useCallback((code: string) => {
    updateCode(code);
  }, [updateCode]);

  // 复制加密密钥
  const handleCopyEncryptionKey = useCallback(async () => {
    if (!encryptionKey) return;

    // 将密钥添加到邀请链接中
    const keyLink = `${window.location.origin}/room/${roomId}?token=${token}&key=${encodeURIComponent(encryptionKey)}`;

    try {
      await navigator.clipboard.writeText(keyLink);
      message.success('带加密密钥的邀请链接已复制到剪贴板');
    } catch {
      // 降级方案
      const textarea = document.createElement('textarea');
      textarea.value = keyLink;
      textarea.style.position = 'fixed';
      textarea.style.opacity = '0';
      document.body.appendChild(textarea);
      textarea.select();
      try {
        document.execCommand('copy');
        message.success('带加密密钥的邀请链接已复制到剪贴板');
      } catch {
        message.success('带加密密钥的邀请链接已生成（请手动复制）');
      }
      document.body.removeChild(textarea);
    }
  }, [encryptionKey, roomId, token]);

  // 获取在线用户列表
  const getAllUsers = useCallback(() => {
    return onlineUsers.map((u) => ({
      id: u.user?.id || '',
      nickname: u.user?.name || '匿名用户',
      color: u.user?.color || '#3b82f6',
      joinedAt: Date.now(), // 注意：这会导致每次重新渲染，但暂时保留
      isCreator: u.user?.id === room?.creatorPeerId,
      isOnline: true,
    }));
  }, [onlineUsers, room]);

  // 加载中
  if (isLoading) {
    return (
      <div className="h-screen flex items-center justify-center bg-white">
        <div className="text-center">
          <div className="w-12 h-12 border-4 border-blue-600 border-t-transparent rounded-full animate-spin mx-auto mb-4" />
          <p className="text-gray-600 font-medium">正在连接房间...</p>
        </div>
      </div>
    );
  }

  // 没有令牌或被拒绝访问
  if (!hasToken || accessDenied) {
    return (
      <div className="h-screen flex items-center justify-center bg-gray-50">
        <div className="text-center bg-white p-8 rounded-xl shadow-lg max-w-md">
          <div className="w-16 h-16 bg-red-100 rounded-full flex items-center justify-center mx-auto mb-4">
            <svg className="w-8 h-8 text-red-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <h2 className="text-xl font-bold text-gray-900 mb-2">访问被拒绝</h2>
          <p className="text-gray-600 mb-4">您没有权限访问此房间</p>
          <p className="text-sm text-gray-500 mb-6">请使用房间创建者分享的邀请链接进入</p>
          <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-3 mb-6 text-left">
            <p className="text-xs text-yellow-800">
              <strong>提示：</strong>邀请链接格式应为：<br />
              <code className="text-xs">http://localhost:3000/join/房间ID?token=xxx</code>
            </p>
          </div>
          <button
            onClick={() => router.push('/')}
            className="px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors font-medium"
          >
            返回首页
          </button>
        </div>
      </div>
    );
  }

  // 房间未加载完成
  if (!room || !nickname) {
    return (
      <div className="h-screen flex items-center justify-center bg-white">
        <div className="text-center">
          <div className="w-12 h-12 border-4 border-blue-600 border-t-transparent rounded-full animate-spin mx-auto mb-4" />
          <p className="text-gray-600 font-medium">正在加载房间...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen bg-white flex flex-col">
      {/* 房间头部 */}
      <RoomHeader
        roomId={roomId}
        roomName={room.name}
        ttl={room.ttl}
        remainingTime={remainingTime}
        onlineCount={getAllUsers().length}
        isCreator={isCreator}
        inviteLink={inviteLink}
        encryptionEnabled={encryptionEnabledState && hasEncryption}
        encryptionKeyString={encryptionKey}
        onGenerateInvite={handleGenerateInvite}
        onDestroyRoom={handleDestroyRoom}
        onCopyEncryptionKey={handleCopyEncryptionKey}
      />

      {/* 房间布局 */}
      <RoomLayout
        leftPanel={<UserList users={getAllUsers()} currentUserId={peerId} />}
        chatPanel={
          <ChatBox
            messages={encryptionEnabledState && hasEncryption ? decryptedMessages : chatMessages}
            currentUserId={yjsPeerId}
            onSendMessage={handleSendMessage}
            onlineUsers={onlineUsers}
            encryptionEnabled={encryptionEnabledState && hasEncryption}
          />
        }
        whiteboardPanel={
          <Canvas
            roomId={roomId}
            userId={peerId}
            userName={nickname}
            yjs={yjs}
          />
        }
        editorPanel={
          <MonacoEditor
            roomId={roomId}
            userId={peerId}
            userName={nickname}
            initialCode={currentCode}
            onCodeChange={handleCodeChange}
            otherUsers={onlineUsers
              .filter(u => u.user?.id !== peerId)
              .map(u => ({
                id: u.user!.id,
                name: u.user!.name,
                color: u.user!.color,
                selection: u.selection,
              }))}
          />
        }
      />

      {/* 昵称输入 Modal */}
      <Modal
        title="加入房间"
        open={nicknameModalOpen}
        onOk={handleNicknameConfirm}
        onCancel={handleNicknameCancel}
        okText="加入"
        cancelText="取消"
        centered
        maskClosable={false}
      >
        <div>
          <p className="mb-4 text-gray-600">请输入您的昵称</p>
          <input
            type="text"
            placeholder="您的昵称"
            value={nicknameInput}
            onChange={(e) => setNicknameInput(e.target.value)}
            className="w-full px-4 py-2 bg-white border border-gray-300 rounded-lg focus:outline-none focus:border-blue-500"
            autoFocus
            maxLength={20}
          />
        </div>
      </Modal>

      {/* 房间销毁通知 Modal */}
      <Modal
        title={getDestroyReasonText(destroyReason).title}
        open={destroyModalOpen}
        onOk={handleDestroyConfirm}
        okText="返回首页"
        cancelButtonProps={{ style: { display: 'none' } }}
        centered
        maskClosable={false}
      >
        <p>{getDestroyReasonText(destroyReason).content}</p>
      </Modal>
    </div>
  );
}
