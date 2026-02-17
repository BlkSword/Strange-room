"use client";

import React, { useState, useRef } from "react";
import Image from "next/image";
import { Button, Modal, Input, Radio, Space, message } from "antd";
import {
  Rocket,
  Plus,
  LogIn,
  Clock,
  ArrowRight,
  Lock,
  UserX,
  Upload
} from "lucide-react";
import { useRouter } from "next/navigation";
import { RoomTTL, type IdleTimeout } from "@/types/room";
import { createRoom as apiCreateRoom, generateToken } from "@/lib/server/api";
import { generateStableIdFromNickname } from "@/lib/utils/hash";
import Link from "next/link";
import { readExportFile, validateExportData, getExportInfo, decryptExportData } from "@/lib/export";

export default function Home() {
  const router = useRouter();
  const [createModalVisible, setCreateModalVisible] = useState(false);
  const [joinModalVisible, setJoinModalVisible] = useState(false);
  const [importModalVisible, setImportModalVisible] = useState(false);
  const [roomName, setRoomName] = useState('');
  const [nickname, setNickname] = useState('');
  const [joinRoomId, setJoinRoomId] = useState('');
  const [joinNickname, setJoinNickname] = useState('');
  const [importNickname, setImportNickname] = useState('');
  const [importFile, setImportFile] = useState<File | null>(null);
  const [importPassword, setImportPassword] = useState('');
  const [ttl, setTtl] = useState<RoomTTL>(24);
  const [idleTimeout, setIdleTimeout] = useState<IdleTimeout>(15);
  const [isCreating, setIsCreating] = useState(false);
  const [isJoining, setIsJoining] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [createDisabled, setCreateDisabled] = useState(false);
  const [joinDisabled, setJoinDisabled] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // 创建房间
  const handleCreateRoom = async () => {
    // 验证昵称
    if (!nickname.trim()) {
      message.warning('请输入您的昵称');
      // 聚焦到昵称输入框
      const nicknameInput = document.querySelector('input[placeholder*="张三"]') as HTMLInputElement;
      nicknameInput?.focus();
      return;
    }

    if (createDisabled) return;

    setCreateDisabled(true);
    setIsCreating(true);

    message.loading('正在创建房间...', 0);

    const startTime = Date.now();
    const MIN_LOADING_TIME = 800;

    try {
      const createResult = await apiCreateRoom(ttl, nickname, idleTimeout);

      if (!createResult.success || !createResult.roomId) {
        message.destroy();
        message.error(createResult.error || '创建房间失败');
        setIsCreating(false);
        setCreateDisabled(false);
        return;
      }

      const roomId = createResult.roomId;
      const tokenResult = await generateToken(roomId);

      if (!tokenResult.success || !tokenResult.token) {
        message.destroy();
        message.error('生成访问令牌失败');
        setIsCreating(false);
        setCreateDisabled(false);
        return;
      }

      const token = tokenResult.token;
      const creatorPeerId = generateStableIdFromNickname(nickname, roomId);

      const now = Date.now();
      const roomData = {
        id: roomId,
        name: roomName || `房间 ${roomId}`,
        ttl,
        idleTimeout,
        createdAt: now,
        expiresAt: createResult.expiresAt || now + ttl * 60 * 60 * 1000,
        lastActiveAt: now,
        creatorPeerId,
        peers: {},
        destroyed: false,
        creator: nickname,
      };

      localStorage.setItem(`room-data-${roomId}`, JSON.stringify(roomData));
      localStorage.setItem(`user-nickname-${roomId}`, nickname);
      localStorage.setItem(`room-token-${roomId}`, token);

      const elapsedTime = Date.now() - startTime;
      if (elapsedTime < MIN_LOADING_TIME) {
        await new Promise(resolve => setTimeout(resolve, MIN_LOADING_TIME - elapsedTime));
      }

      message.destroy();
      message.success('房间创建成功！正在跳转...', 2);
      // 延迟跳转，让用户看到成功消息
      await new Promise(resolve => setTimeout(resolve, 1000));
      router.push(`/room/${roomId}?token=${token}`);
    } catch (error) {
      console.error('[Home] 创建房间失败:', error);
      message.destroy();
      message.error('创建房间失败，请检查网络连接');
      setIsCreating(false);
      setCreateDisabled(false);
    }
  };

  // 加入房间
  const handleJoinRoom = async () => {
    // 验证输入
    if (!joinRoomId.trim()) {
      message.warning('请输入房间ID');
      const roomIdInput = document.querySelector('input[placeholder*="ABC123"]') as HTMLInputElement;
      roomIdInput?.focus();
      return;
    }

    if (!joinNickname.trim()) {
      message.warning('请输入您的昵称');
      const nicknameInput = document.querySelectorAll('input[placeholder*="张三"]')[1] as HTMLInputElement;
      nicknameInput?.focus();
      return;
    }

    if (joinDisabled) return;

    setJoinDisabled(true);
    setIsJoining(true);

    message.loading('正在加入房间...', 0);

    try {
      // 生成访问令牌
      const tokenResult = await generateToken(joinRoomId.toUpperCase());

      if (!tokenResult.success || !tokenResult.token) {
        message.destroy();
        message.error('房间不存在或已过期，请检查房间ID');
        setIsJoining(false);
        setJoinDisabled(false);
        return;
      }

      const token = tokenResult.token;

      localStorage.setItem(`user-nickname-${joinRoomId.toUpperCase()}`, joinNickname);
      localStorage.setItem(`room-token-${joinRoomId.toUpperCase()}`, token);

      // 稍微延迟让用户看到加载状态
      await new Promise(resolve => setTimeout(resolve, 500));

      message.destroy();
      message.success('加入成功！正在跳转...', 2);
      // 再延迟一下，让用户看到成功消息
      await new Promise(resolve => setTimeout(resolve, 800));
      router.push(`/room/${joinRoomId.toUpperCase()}?token=${token}`);
    } catch (error) {
      console.error('[Home] 加入房间失败:', error);
      message.destroy();
      message.error('加入房间失败，请检查网络连接和房间ID');
      setIsJoining(false);
      setJoinDisabled(false);
    }
  };

  // 处理文件选择
  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      setImportFile(file);
    }
  };

  // 导入房间数据
  const handleImportRoom = async () => {
    // 验证昵称
    if (!importNickname.trim()) {
      message.warning('请输入您的昵称');
      return;
    }

    if (!importFile) {
      message.warning('请选择导出文件');
      return;
    }

    setIsImporting(true);
    message.loading('正在处理导入数据...', 0);

    try {
      // 读取文件
      const data = await readExportFile(importFile);

      // 验证数据格式
      if (!validateExportData(data)) {
        message.destroy();
        message.error('无效的导出文件格式');
        setIsImporting(false);
        return;
      }

      // 获取文件信息
      const info = getExportInfo(data);

      // 如果是加密文件，验证密码
      let roomData = data;
      if (info.encrypted) {
        if (!importPassword.trim()) {
          message.destroy();
          message.warning('请输入解密密码');
          setIsImporting(false);
          return;
        }
        try {
          roomData = decryptExportData(data as any, importPassword);
        } catch {
          message.destroy();
          message.error('密码错误或文件已损坏');
          setIsImporting(false);
          return;
        }
      }

      message.destroy();
      message.success('数据解析成功，正在创建房间...');

      // 创建新房间
      const createResult = await apiCreateRoom(24, importNickname, 15);

      if (!createResult.success || !createResult.roomId) {
        message.error(createResult.error || '创建房间失败');
        setIsImporting(false);
        return;
      }

      const roomId = createResult.roomId;
      const tokenResult = await generateToken(roomId);

      if (!tokenResult.success || !tokenResult.token) {
        message.error('生成访问令牌失败');
        setIsImporting(false);
        return;
      }

      const token = tokenResult.token;
      const creatorPeerId = generateStableIdFromNickname(importNickname, roomId);

      const now = Date.now();

      // 使用导入的房间数据
      const importRoomData = roomData as any;

      const newRoomData = {
        id: roomId,
        name: importRoomData.roomName || `房间 ${roomId}`,
        ttl: importRoomData.metadata.ttl || 24,
        idleTimeout: importRoomData.metadata.idleTimeout || 15,
        createdAt: now,
        expiresAt: now + (importRoomData.metadata.ttl || 24) * 60 * 60 * 1000,
        lastActiveAt: now,
        creatorPeerId,
        peers: {},
        destroyed: false,
        creator: importNickname,
      };

      // 保存房间数据到 localStorage
      localStorage.setItem(`room-data-${roomId}`, JSON.stringify(newRoomData));
      localStorage.setItem(`user-nickname-${roomId}`, importNickname);
      localStorage.setItem(`room-token-${roomId}`, token);

      // 如果有加密密钥，也保存
      if (importRoomData.userData?.encryptionKey) {
        localStorage.setItem(`room-encryption-key-${roomId}`, importRoomData.userData.encryptionKey);
      }
      if (importRoomData.userData?.encryptionEnabled) {
        localStorage.setItem(`room-encryption-enabled-${roomId}`, 'true');
      }

      // 保存导入数据到 sessionStorage，在房间页面还原
      sessionStorage.setItem(`import-data-${roomId}`, JSON.stringify(importRoomData.yjsData));

      // 跳转到房间页面
      await new Promise(resolve => setTimeout(resolve, 500));
      router.push(`/room/${roomId}?token=${token}&import=true`);
    } catch (error) {
      console.error('[Home] 导入房间失败:', error);
      message.destroy();
      message.error('导入失败：' + (error as Error).message);
      setIsImporting(false);
    }
  };

  // 打开导入弹窗
  const handleOpenImportModal = () => {
    setImportNickname('');
    setImportFile(null);
    setImportPassword('');
    setImportModalVisible(true);
  };

  // 关闭导入弹窗
  const handleCloseImportModal = () => {
    if (!isImporting) {
      setImportModalVisible(false);
      setImportNickname('');
      setImportFile(null);
      setImportPassword('');
    }
  };

  return (
    <div className="min-h-screen bg-sketch-background flex flex-col">
      {/* 顶部导航 */}
      <nav className="border-b-2 border-sketch-black bg-sketch-card">
        <div className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Image
              src="/favicon.ico"
              alt="Strange Room"
              width={24}
              height={24}
            />
            <span className="text-2xl font-bold font-marker text-sketch-black">
              Strange Room
            </span>
          </div>

          <div className="flex items-center gap-6">
          </div>
        </div>
      </nav>

      {/* 主内容区 */}
      <main className="flex-1 flex items-center justify-center px-6 py-12">
        <div className="w-full max-w-5xl">
          {/* 欢迎标语 */}
          <div className="text-center mb-12">
            <div className="inline-flex items-center gap-2 px-6 py-3 rounded-sketch bg-sketch-light border-2 border-sketch-black text-sketch-black font-cave text-lg mb-6">
              <span>端到端加密 · 自动销毁 · 无需注册</span>
            </div>
            <h1 className="text-5xl md:text-6xl font-marker mb-4 leading-tight hand-drawn-title">
              临时协作空间
            </h1>
            <p className="text-xl text-sketch-gray font-cave">
              创建或加入房间，开始即时协作
            </p>
          </div>

          {/* 功能卡片 */}
          <div className="grid md:grid-cols-3 gap-6">
            {/* 创建房间卡片 */}
            <div className="mystic-card hand-drawn-border hover:translate-y-[-4px] transition-transform duration-200">
              <div className="text-center">
                <div className="w-16 h-16 rounded-sketch bg-sketch-light flex items-center justify-center mx-auto mb-4 border-2 border-sketch-black">
                  <Plus size={32} className="text-sketch-black" />
                </div>
                <h2 className="text-2xl font-semibold text-sketch-black mb-3 font-cave">创建房间</h2>
                <p className="text-sketch-gray mb-6 font-cave">
                  创建一个新的协作房间
                </p>
                <div className="space-y-3 mb-6 text-left">
                  <div className="flex items-center gap-2 text-sm text-sketch-gray font-cave">
                    <Clock size={16} className="text-sketch-accent" />
                    <span>自定义有效期</span>
                  </div>
                  <div className="flex items-center gap-2 text-sm text-sketch-gray font-cave">
                    <Lock size={16} className="text-sketch-accent" />
                    <span>可选端到端加密</span>
                  </div>
                </div>
                <Button
                  type="primary"
                  size="large"
                  icon={<Rocket size={20} />}
                  onClick={() => setCreateModalVisible(true)}
                  className="hand-drawn-btn text-lg px-8 w-full"
                >
                  创建
                </Button>
              </div>
            </div>

            {/* 加入房间卡片 */}
            <div className="mystic-card hand-drawn-border hover:translate-y-[-4px] transition-transform duration-200">
              <div className="text-center">
                <div className="w-16 h-16 rounded-sketch bg-sketch-light flex items-center justify-center mx-auto mb-4 border-2 border-sketch-black">
                  <LogIn size={32} className="text-sketch-black" />
                </div>
                <h2 className="text-2xl font-semibold text-sketch-black mb-3 font-cave">加入房间</h2>
                <p className="text-sketch-gray mb-6 font-cave">
                  通过房间ID加入现有房间
                </p>
                <div className="space-y-3 mb-6 text-left">
                  <div className="flex items-center gap-2 text-sm text-sketch-gray font-cave">
                    <LogIn size={16} className="text-sketch-accent" />
                    <span>输入房间ID即可加入</span>
                  </div>
                  <div className="flex items-center gap-2 text-sm text-sketch-gray font-cave">
                    <Lock size={16} className="text-sketch-accent" />
                    <span>支持加密房间</span>
                  </div>
                </div>
                <Button
                  type="primary"
                  size="large"
                  icon={<LogIn size={20} />}
                  onClick={() => setJoinModalVisible(true)}
                  className="hand-drawn-btn text-lg px-8 w-full"
                >
                  加入
                </Button>
              </div>
            </div>

            {/* 导入房间卡片 */}
            <div className="mystic-card hand-drawn-border hover:translate-y-[-4px] transition-transform duration-200">
              <div className="text-center">
                <div className="w-16 h-16 rounded-sketch bg-sketch-light flex items-center justify-center mx-auto mb-4 border-2 border-sketch-black">
                  <Upload size={32} className="text-sketch-black" />
                </div>
                <h2 className="text-2xl font-semibold text-sketch-black mb-3 font-cave">导入数据</h2>
                <p className="text-sketch-gray mb-6 font-cave">
                  从导出文件恢复房间数据
                </p>
                <div className="space-y-3 mb-6 text-left">
                  <div className="flex items-center gap-2 text-sm text-sketch-gray font-cave">
                    <Upload size={16} className="text-sketch-accent" />
                    <span>完整的房间数据还原</span>
                  </div>
                  <div className="flex items-center gap-2 text-sm text-sketch-gray font-cave">
                    <Lock size={16} className="text-sketch-accent" />
                    <span>支持加密文件导入</span>
                  </div>
                </div>
                <Button
                  type="primary"
                  size="large"
                  icon={<Upload size={20} />}
                  onClick={handleOpenImportModal}
                  className="hand-drawn-btn text-lg px-8 w-full"
                >
                  导入
                </Button>
              </div>
            </div>
          </div>

          {/* 特性提示 */}
          <div className="mt-12 text-center">
            <p className="text-sketch-gray font-cave text-base">
              支持协作白板 · 代码编辑 · 实时聊天
            </p>
          </div>
        </div>
      </main>

      {/* 页脚 */}
      <footer className="py-6 border-t-2 border-sketch-light bg-sketch-card">
        <div className="max-w-6xl mx-auto">
          <div className="flex flex-col md:flex-row items-center justify-between gap-4">
            <div className="flex items-center gap-3">
              <span className="font-semibold text-sketch-black font-cave">Strange Room</span>
            </div>

            <div className="flex items-center gap-6 text-sm text-sketch-gray font-cave">
              <Link href="/about" className="hover:text-sketch-black">关于</Link>
              <a href="https://github.com/BlkSword/Strange-room" target="_blank" rel="noopener noreferrer" className="hover:text-sketch-black">
                GitHub
              </a>
            </div>

            <div className="text-sm text-sketch-gray font-cave">
              Apache-2.0 开源协议
            </div>
          </div>
        </div>
      </footer>

      {/* 创建房间弹窗 */}
      <Modal
        title={
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 rounded-sketch bg-sketch-light flex items-center justify-center border-2 border-sketch-black">
              <Rocket size={24} className="text-sketch-black" />
            </div>
            <span className="text-xl font-semibold text-sketch-black font-cave">创建临时房间</span>
          </div>
        }
        open={createModalVisible}
        onOk={handleCreateRoom}
        onCancel={() => !isCreating && setCreateModalVisible(false)}
        okText={<span className="flex items-center gap-2 font-cave">{isCreating ? '创建中...' : '创建房间'} {!isCreating && <ArrowRight size={16} />}</span>}
        cancelText="取消"
        width={500}
        styles={{ body: { padding: '24px' } }}
        confirmLoading={isCreating}
        okButtonProps={{ disabled: createDisabled }}
        maskClosable={!isCreating}
        closable={!isCreating}
      >
        <Space direction="vertical" size="large" className="w-full">
          <div>
            <label className="block text-base font-cave text-sketch-black mb-2">房间名称（可选）</label>
            <Input
              value={roomName}
              onChange={(e) => setRoomName(e.target.value)}
              maxLength={30}
              size="large"
            />
          </div>

          <div>
            <label className="block text-base font-cave text-sketch-black mb-2">你的昵称</label>
            <Input
              value={nickname}
              onChange={(e) => setNickname(e.target.value)}
              maxLength={20}
              size="large"
            />
          </div>

          <div>
            <label className="block text-base font-cave text-sketch-black mb-3">房间有效期</label>
            <Radio.Group
              value={ttl}
              onChange={(e) => setTtl(e.target.value)}
              className="w-full"
            >
              <div className="grid grid-cols-4 gap-3">
                {[
                  { value: 1, label: '1 小时' },
                  { value: 6, label: '6 小时' },
                  { value: 24, label: '24 小时' },
                  { value: 48, label: '48 小时' }
                ].map((option) => (
                  <Radio.Button
                    key={option.value}
                    value={option.value}
                    className={`w-full text-center py-3 rounded-sketch border-2 font-cave ${ttl === option.value
                      ? 'bg-sketch-black border-sketch-black text-sketch-background'
                      : 'border-sketch-gray hover:border-sketch-black bg-sketch-card'
                      }`}
                  >
                    {option.label}
                  </Radio.Button>
                ))}
              </div>
            </Radio.Group>
            <p className="text-sm text-sketch-gray mt-3 flex items-center gap-2 font-cave">
              <Clock size={16} />
              有效期结束后，房间将自动销毁，所有数据将被永久删除
            </p>
          </div>

          <div>
            <label className="block text-base font-cave text-sketch-black mb-3">空闲超时</label>
            <Radio.Group
              value={idleTimeout}
              onChange={(e) => setIdleTimeout(e.target.value)}
              className="w-full"
            >
              <div className="grid grid-cols-4 gap-3">
                {[
                  { value: 5, label: '5 分钟' },
                  { value: 15, label: '15 分钟' },
                  { value: 30, label: '30 分钟' },
                  { value: 60, label: '1 小时' }
                ].map((option) => (
                  <Radio.Button
                    key={option.value}
                    value={option.value}
                    className={`w-full text-center py-3 rounded-sketch border-2 font-cave ${idleTimeout === option.value
                      ? 'bg-sketch-black border-sketch-black text-sketch-background'
                      : 'border-sketch-gray hover:border-sketch-black bg-sketch-card'
                      }`}
                  >
                    {option.label}
                  </Radio.Button>
                ))}
              </div>
            </Radio.Group>
            <p className="text-sm text-sketch-gray mt-3 flex items-center gap-2 font-cave">
              <UserX size={16} />
              当房间内没有任何在线用户超过此时间，房间将自动销毁
            </p>
          </div>
        </Space>
      </Modal>

      {/* 加入房间弹窗 */}
      <Modal
        title={
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 rounded-sketch bg-sketch-light flex items-center justify-center border-2 border-sketch-black">
              <Rocket size={24} className="text-sketch-black" />
            </div>
            <span className="text-xl font-semibold text-sketch-black font-cave">加入房间</span>
          </div>
        }
        open={joinModalVisible}
        onOk={handleJoinRoom}
        onCancel={() => !isJoining && setJoinModalVisible(false)}
        okText={<span className="flex items-center gap-2 font-cave">{isJoining ? '加入中...' : '加入房间'} {!isJoining && <ArrowRight size={16} />}</span>}
        cancelText="取消"
        width={450}
        styles={{ body: { padding: '24px' } }}
        confirmLoading={isJoining}
        okButtonProps={{ disabled: joinDisabled }}
        maskClosable={!isJoining}
        closable={!isJoining}
      >
        <Space direction="vertical" size="large" className="w-full">
          <div>
            <label className="block text-base font-cave text-sketch-black mb-2">房间ID</label>
            <Input
              placeholder="ABC123"
              value={joinRoomId}
              onChange={(e) => setJoinRoomId(e.target.value.toUpperCase())}
              maxLength={6}
              size="large"
              className="font-mono"
            />
            <p className="text-sm text-sketch-gray mt-2 font-cave">请输入房间创建者分享的6位房间ID</p>
          </div>

          <div>
            <label className="block text-base font-cave text-sketch-black mb-2">你的昵称</label>
            <Input
              placeholder="ROOM"
              value={joinNickname}
              onChange={(e) => setJoinNickname(e.target.value)}
              maxLength={20}
              size="large"
            />
          </div>

          <div className="flex items-start gap-2 p-4 bg-sketch-light rounded-sketch border-2 border-sketch-black">
            <Lock size={18} className="text-sketch-black flex-shrink-0 mt-0.5" />
            <div className="text-sm text-sketch-gray font-cave">
              <p className="font-medium mb-1 text-sketch-black">安全提示</p>
              <p>请确保从可信来源获取房间ID。加入房间后，你将能够看到房间内的所有内容。</p>
            </div>
          </div>
        </Space>
      </Modal>

      {/* 导入房间数据弹窗 */}
      <Modal
        title={
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 rounded-sketch bg-sketch-light flex items-center justify-center border-2 border-sketch-black">
              <Upload size={24} className="text-sketch-black" />
            </div>
            <span className="text-xl font-semibold text-sketch-black font-cave">导入房间数据</span>
          </div>
        }
        open={importModalVisible}
        onOk={handleImportRoom}
        onCancel={handleCloseImportModal}
        okText={<span className="flex items-center gap-2 font-cave">{isImporting ? '导入中...' : '导入数据'} {!isImporting && <ArrowRight size={16} />}</span>}
        cancelText="取消"
        width={500}
        styles={{ body: { padding: '24px' } }}
        confirmLoading={isImporting}
        okButtonProps={{ disabled: !importFile }}
        maskClosable={!isImporting}
        closable={!isImporting}
      >
        <Space direction="vertical" size="large" className="w-full">
          <div>
            <label className="block text-base font-cave text-sketch-black mb-2">你的昵称</label>
            <Input
              placeholder="请输入您的昵称"
              value={importNickname}
              onChange={(e) => setImportNickname(e.target.value)}
              maxLength={20}
              size="large"
            />
          </div>

          <div>
            <label className="block text-base font-cave text-sketch-black mb-2">选择导出文件</label>
            <input
              ref={fileInputRef}
              type="file"
              accept=".json"
              onChange={handleFileChange}
              className="w-full px-3 py-2 bg-white border border-gray-300 rounded-lg focus:outline-none focus:border-blue-500 font-cave"
            />
            {importFile && (
              <p className="text-sm text-green-600 mt-2 font-cave">已选择: {importFile.name}</p>
            )}
          </div>

          <div>
            <label className="block text-base font-cave text-sketch-black mb-2">解密密码（可选）</label>
            <Input.Password
              placeholder="如果文件已加密，请输入密码"
              value={importPassword}
              onChange={(e) => setImportPassword(e.target.value)}
              size="large"
            />
            <p className="text-sm text-sketch-gray mt-2 font-cave">如果导出时设置了密码，请在此输入</p>
          </div>

          <div className="flex items-start gap-2 p-4 bg-blue-50 rounded-sketch border-2 border-blue-200">
            <Lock size={18} className="text-blue-600 flex-shrink-0 mt-0.5" />
            <div className="text-sm text-sketch-gray font-cave">
              <p className="font-medium mb-1 text-blue-800">导入说明</p>
              <p>导入将创建一个新房间并还原所有数据，包括聊天记录、代码和白板内容。</p>
            </div>
          </div>
        </Space>
      </Modal>
    </div>
  );
}
