"use client";

import React, { useState } from "react";
import { Button, Modal, Input, Radio, Card, Space, message } from "antd";
import {
  Rocket,
  CheckCircle,
  Shield,
  Zap,
  Users,
  Code,
  PenTool,
  Clock,
  ArrowRight,
  Github,
  Lock,
  Server,
  Eye,
  Globe
} from "lucide-react";
import { useRouter } from "next/navigation";
import { RoomTTL } from "@/types/room";
import { createRoom as apiCreateRoom, generateToken } from "@/lib/server/api";
import { generateStableIdFromNickname } from "@/lib/utils/hash";

export default function Home() {
  const router = useRouter();
  const [modalVisible, setModalVisible] = useState(false);
  const [roomName, setRoomName] = useState('');
  const [nickname, setNickname] = useState('');
  const [ttl, setTtl] = useState<RoomTTL>(24);
  const [isCreating, setIsCreating] = useState(false);

  // 创建房间
  const handleCreateRoom = async () => {
    if (!nickname.trim()) {
      message.warning('请输入您的昵称');
      return;
    }

    setIsCreating(true);

    try {
      // 1. 调用 API 创建房间
      const createResult = await apiCreateRoom(ttl, nickname);

      if (!createResult.success || !createResult.roomId) {
        message.error(createResult.error || '创建房间失败');
        setIsCreating(false);
        return;
      }

      const roomId = createResult.roomId;

      // 2. 生成访问令牌（创建者也需要令牌）
      const tokenResult = await generateToken(roomId);

      if (!tokenResult.success || !tokenResult.token) {
        message.error('生成访问令牌失败');
        setIsCreating(false);
        return;
      }

      const token = tokenResult.token;

      // 3. 基于昵称生成稳定的创建者 ID
      const creatorPeerId = generateStableIdFromNickname(nickname, roomId);

      // 4. 保存到 localStorage（用于返回时使用）
      const now = Date.now();
      const roomData = {
        id: roomId,
        name: roomName || `房间 ${roomId}`,
        ttl,
        createdAt: now,
        expiresAt: createResult.expiresAt || now + ttl * 60 * 60 * 1000,
        creatorPeerId,
        peers: {},
        destroyed: false,
        creator: nickname,
      };

      localStorage.setItem(`room-data-${roomId}`, JSON.stringify(roomData));
      localStorage.setItem(`user-nickname-${roomId}`, nickname);
      localStorage.setItem(`room-token-${roomId}`, token);

      message.success('房间创建成功！');

      // 5. 跳转到房间页面（带 token 参数）
      router.push(`/room/${roomId}?token=${token}`);
    } catch (error) {
      console.error('[Home] 创建房间失败:', error);
      message.error('创建房间失败，请检查网络连接');
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <div className="min-h-screen bg-white">
      {/* 顶部导航 */}
      <nav className="fixed top-0 left-0 right-0 z-50 bg-white/80 backdrop-blur-lg border-b border-gray-200">
        <div className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-600 to-indigo-600 flex items-center justify-center">
              <Code size={18} className="text-white" />
            </div>
            <span className="text-xl font-bold text-gray-900">Strange Room</span>
          </div>

          <div className="flex items-center gap-6">
            <a href="#features" className="text-sm text-gray-600 hover:text-gray-900 transition-colors">功能</a>
            <a href="#architecture" className="text-sm text-gray-600 hover:text-gray-900 transition-colors">架构</a>
            <a href="#security" className="text-sm text-gray-600 hover:text-gray-900 transition-colors">安全</a>
            <a href="https://github.com/BlkSword/Strange-room" target="_blank" rel="noopener noreferrer" className="text-gray-600 hover:text-gray-900">
              <Github size={18} />
            </a>
            <Button
              type="primary"
              icon={<Rocket size={16} />}
              onClick={() => setModalVisible(true)}
              className="h-9 px-5 bg-blue-600 hover:bg-blue-700 border-none rounded-lg font-medium"
            >
              创建房间
            </Button>
          </div>
        </div>
      </nav>

      {/* Hero 区域 */}
      <section className="pt-32 pb-20 px-6">
        <div className="max-w-5xl mx-auto text-center">
          <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-blue-50 text-blue-700 text-sm font-medium mb-8">
            <Zap size={16} />
            <span>端到端加密 · 自动销毁 · 无需注册</span>
          </div>

          <h1 className="text-5xl md:text-6xl font-bold text-gray-900 mb-6 leading-tight">
            临时协作空间
            <br />
            <span className="text-blue-600">用完即焚</span>
          </h1>

          <p className="text-xl text-gray-600 mb-10 max-w-2xl mx-auto leading-relaxed">
            为黑客松、远程面试、临时讨论设计的协作空间。
            <br />
            白板、代码、聊天 —— 有效期结束后自动销毁。
          </p>

          <div className="flex items-center justify-center gap-4">
            <Button
              type="primary"
              size="large"
              icon={<Rocket size={18} />}
              onClick={() => setModalVisible(true)}
              className="h-12 px-8 bg-blue-600 hover:bg-blue-700 border-none rounded-xl font-semibold text-base shadow-lg shadow-blue-600/30"
            >
              立即创建房间
            </Button>
          </div>

          {/* 信任指标 */}
          <div className="flex items-center justify-center gap-8 mt-16 pt-8 border-t border-gray-200">
            <div className="text-center">
              <div className="text-2xl font-bold text-gray-900">0</div>
              <div className="text-sm text-gray-500">服务器存储</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-gray-900">100%</div>
              <div className="text-sm text-gray-500">端到端加密</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-gray-900">48h</div>
              <div className="text-sm text-gray-500">最长有效期</div>
            </div>
          </div>
        </div>
      </section>

      {/* 核心功能 */}
      <section id="features" className="py-20 px-6 bg-gray-50">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-3xl font-bold text-gray-900 mb-4">三大核心功能</h2>
            <p className="text-gray-600">简单而强大的协作工具，无需学习即可上手</p>
          </div>

          <div className="grid md:grid-cols-3 gap-8">
            {[
              {
                icon: <PenTool size={32} className="text-blue-600" />,
                title: "协作白板",
                desc: "多人实时涂鸦，支持画笔、形状、橡皮擦。就像在真实白板上一样直观。",
                features: ["自由绘制", "多种画笔工具", "多人协同", "导出图片"]
              },
              {
                icon: <Code size={32} className="text-purple-600" />,
                title: "代码协同",
                desc: "内置 Monaco 编辑器，支持 JavaScript/Python/Go 等多种语言，实时协同编程。",
                features: ["语法高亮", "自动补全", "多人编辑", "光标追踪"]
              },
              {
                icon: <Clock size={32} className="text-amber-600" />,
                title: "自动销毁",
                desc: "1/6/24/48小时可选，倒计时结束后所有数据永久删除，不留任何痕迹。",
                features: ["定时销毁", "手动删除", "数据清除", "隐私保护"]
              }
            ].map((feature, i) => (
              <Card
                key={i}
                className="border-0 shadow-sm hover:shadow-md transition-shadow bg-white"
                styles={{ body: { padding: '32px' } }}
              >
                <div className="w-14 h-14 rounded-xl bg-blue-50 flex items-center justify-center mb-6">
                  {feature.icon}
                </div>
                <h3 className="text-xl font-semibold text-gray-900 mb-3">{feature.title}</h3>
                <p className="text-gray-600 leading-relaxed mb-4">{feature.desc}</p>
                <ul className="space-y-2">
                  {feature.features.map((f, j) => (
                    <li key={j} className="flex items-center gap-2 text-sm text-gray-500">
                      <CheckCircle size={14} className="text-green-500" />
                      {f}
                    </li>
                  ))}
                </ul>
              </Card>
            ))}
          </div>
        </div>
      </section>

      {/* 技术架构 */}
      <section id="architecture" className="py-20 px-6">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-3xl font-bold text-gray-900 mb-4">技术架构</h2>
            <p className="text-gray-600">基于 WebRTC 的纯 P2P 架构，数据不经过服务器</p>
          </div>

          <div className="grid md:grid-cols-3 gap-8">
            {[
              {
                icon: <Server size={28} className="text-blue-600" />,
                title: "信令服务器",
                desc: "仅负责建立 WebRTC 连接，不存储任何业务数据。连接建立后即退出，无法窥探通信内容。",
                tech: ["Node.js", "WebSocket", "PeerJS"]
              },
              {
                icon: <Users size={28} className="text-purple-600" />,
                title: "P2P 通信",
                desc: "数据直接在用户设备间传输，通过 WebRTC 建立加密通道，确保通信安全和低延迟。",
                tech: ["WebRTC", "DataChannel", "SRTP"]
              },
              {
                icon: <Eye size={28} className="text-amber-600" />,
                title: "本地存储",
                desc: "房间数据存储在浏览器 IndexedDB 中，本地加密，房间销毁后自动清除。",
                tech: ["IndexedDB", "Yjs", "CRDT"]
              }
            ].map((item, i) => (
              <div key={i} className="bg-gray-50 rounded-xl p-8">
                <div className="flex items-center gap-4 mb-4">
                  <div className="w-12 h-12 rounded-lg bg-white flex items-center justify-center shadow-sm">
                    {item.icon}
                  </div>
                  <h3 className="text-lg font-semibold text-gray-900">{item.title}</h3>
                </div>
                <p className="text-gray-600 mb-4">{item.desc}</p>
                <div className="flex flex-wrap gap-2">
                  {item.tech.map((t, j) => (
                    <span key={j} className="px-3 py-1 bg-white rounded-full text-xs text-gray-600 border border-gray-200">
                      {t}
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* 安全保障 */}
      <section id="security" className="py-20 px-6 bg-gray-50">
        <div className="max-w-6xl mx-auto">
          <div className="grid md:grid-cols-2 gap-12 items-center">
            <div>
              <h2 className="text-3xl font-bold text-gray-900 mb-6">
                隐私优先
                <br />
                <span className="text-blue-600">端到端加密</span>
              </h2>
              <p className="text-lg text-gray-600 mb-8 leading-relaxed">
                所有数据通过 WebRTC 点对点传输，直接在你的设备之间流转。
                我们的服务器仅负责信令交换，无法窥探任何内容。
              </p>

              <div className="space-y-4">
                {[
                  { icon: <Shield size={20} />, title: "P2P 加密传输", desc: "数据不经过服务器存储" },
                  { icon: <Clock size={20} />, title: "自动销毁机制", desc: "到期后永久删除所有记录" },
                  { icon: <Users size={20} />, title: "无需注册登录", desc: "打开即用，不收集任何个人信息" }
                ].map((item, i) => (
                  <div key={i} className="flex items-start gap-4">
                    <div className="flex-shrink-0 w-10 h-10 rounded-lg bg-blue-50 flex items-center justify-center text-blue-600">
                      {item.icon}
                    </div>
                    <div>
                      <h4 className="font-semibold text-gray-900 mb-1">{item.title}</h4>
                      <p className="text-sm text-gray-600">{item.desc}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            <div className="bg-gradient-to-br from-blue-500 to-indigo-600 rounded-2xl p-8 text-white">
              <div className="space-y-6">
                <div className="flex items-center gap-3">
                  <Lock size={24} />
                  <span className="font-semibold">安全架构</span>
                </div>
                <div className="space-y-4 text-blue-100">
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5" />
                    <span>WebRTC 点对点连接，数据直接在用户设备间传输</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5" />
                    <span>信令服务器仅交换连接信息，不存储任何业务数据</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5" />
                    <span>房间数据存储在浏览器 IndexedDB，本地加密</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5" />
                    <span>倒计时结束自动清理，无法恢复</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5" />
                    <span>开源代码，可自行审查和部署</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* 使用场景 */}
      <section className="py-20 px-6">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-3xl font-bold text-gray-900 mb-4">适用场景</h2>
            <p className="text-gray-600">为临时协作而设计，满足各种即时沟通需求</p>
          </div>

          <div className="grid md:grid-cols-2 gap-6">
            {[
              { icon: <Users size={28} />, title: "黑客松协作", desc: "48小时内快速组队，在白板上讨论方案，共享代码片段", tags: ["组队", "讨论", "共享"] },
              { icon: <Code size={28} />, title: "技术面试", desc: "在线白板讲解架构图，Monaco 编辑器演示代码实现", tags: ["面试", "白板", "代码"] },
              { icon: <PenTool size={28} />, title: "临时讨论", desc: "不想留下聊天记录？用完即焚的讨论空间，隐私无忧", tags: ["隐私", "临时", "无痕"] },
              { icon: <Shield size={28} />, title: "敏感信息交换", desc: "端到端加密传输，适合交换 API 密钥、配置等敏感信息", tags: ["加密", "安全", "P2P"] }
            ].map((useCase, i) => (
              <Card
                key={i}
                className="border border-gray-200 hover:border-blue-300 hover:shadow-md transition-all bg-white"
                styles={{ body: { padding: '24px' } }}
              >
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-12 h-12 rounded-xl bg-gray-100 flex items-center justify-center text-gray-700">
                    {useCase.icon}
                  </div>
                  <div className="flex-1">
                    <h3 className="text-lg font-semibold text-gray-900 mb-2">{useCase.title}</h3>
                    <p className="text-gray-600 text-sm mb-4">{useCase.desc}</p>
                    <div className="flex items-center gap-2">
                      {useCase.tags.map((tag, j) => (
                        <span key={j} className="px-2 py-1 bg-gray-100 text-gray-600 text-xs rounded-md">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </div>
                </div>
              </Card>
            ))}
          </div>
        </div>
      </section>

      {/* 开源信息 */}
      <section className="py-20 px-6 bg-gray-50">
        <div className="max-w-4xl mx-auto text-center">
          <div className="inline-flex items-center gap-3 px-6 py-3 rounded-full bg-white border border-gray-200 mb-8">
            <Github size={20} className="text-gray-700" />
            <span className="text-gray-700 font-medium">开源项目</span>
          </div>

          <h2 className="text-3xl font-bold text-gray-900 mb-4">代码完全开源</h2>
          <p className="text-gray-600 mb-8 max-w-2xl mx-auto">
            Strange Room 是一个开源项目，你可以自由查看、修改和部署代码。
            所有代码都在 GitHub 上公开，欢迎贡献和反馈。
          </p>

          <div className="flex items-center justify-center gap-8 mb-8">
            <div className="text-center">
              <div className="text-2xl font-bold text-gray-900">Next.js</div>
              <div className="text-sm text-gray-500">前端框架</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-gray-900">WebRTC</div>
              <div className="text-sm text-gray-500">P2P 通信</div>
            </div>
            <div className="text-center">
              <div className="text-2xl font-bold text-gray-900">Yjs</div>
              <div className="text-sm text-gray-500">协同编辑</div>
            </div>
          </div>

          <Button
            type="primary"
            size="large"
            icon={<Github size={18} />}
            onClick={() => window.open('https://github.com/BlkSword/Strange-room', '_blank')}
            className="h-12 px-8 bg-gray-900 hover:bg-gray-800 border-none rounded-xl font-semibold"
          >
            查看 GitHub 仓库
          </Button>
        </div>
      </section>

      {/* CTA 区域 */}
      <section className="py-20 px-6">
        <div className="max-w-3xl mx-auto text-center">
          <h2 className="text-3xl font-bold text-gray-900 mb-4">准备好开始协作了吗？</h2>
          <p className="text-gray-600 mb-8">无需注册，打开即用。创建你的第一个临时协作空间。</p>
          <Button
            type="primary"
            size="large"
            icon={<Rocket size={18} />}
            onClick={() => setModalVisible(true)}
            className="h-12 px-8 bg-blue-600 hover:bg-blue-700 border-none rounded-xl font-semibold"
          >
            创建免费房间
          </Button>
        </div>
      </section>

      {/* 页脚 */}
      <footer className="py-12 px-6 border-t border-gray-200">
        <div className="max-w-6xl mx-auto">
          <div className="flex flex-col md:flex-row items-center justify-between gap-6">
            <div className="flex items-center gap-3">
              <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-600 to-indigo-600 flex items-center justify-center">
                <Code size={18} className="text-white" />
              </div>
              <span className="font-semibold text-gray-900">Strange Room</span>
            </div>

            <div className="flex items-center gap-6 text-sm text-gray-600">
              <a href="https://github.com/BlkSword/Strange-room" target="_blank" rel="noopener noreferrer" className="hover:text-gray-900">
                GitHub
              </a>
              <a href="#" className="hover:text-gray-900">
                隐私政策
              </a>
              <a href="#" className="hover:text-gray-900">
                使用条款
              </a>
            </div>

            <div className="text-sm text-gray-500">
              遵循 Apache-2.0 开源协议
            </div>
          </div>
        </div>
      </footer>

      {/* 创建房间弹窗 */}
      <Modal
        title={
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-lg bg-blue-100 flex items-center justify-center">
              <Rocket size={20} className="text-blue-600" />
            </div>
            <span className="text-lg font-semibold">创建临时房间</span>
          </div>
        }
        open={modalVisible}
        onOk={handleCreateRoom}
        onCancel={() => !isCreating && setModalVisible(false)}
        okText={<span className="flex items-center gap-2">{isCreating ? '创建中...' : '创建房间'} {!isCreating && <ArrowRight size={16} />}</span>}
        cancelText="取消"
        width={480}
        styles={{ body: { padding: '24px' } }}
        confirmLoading={isCreating}
        maskClosable={!isCreating}
        closable={!isCreating}
      >
        <Space direction="vertical" size="large" className="w-full">
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">房间名称（可选）</label>
            <Input
              placeholder="例如：黑客松团队A"
              value={roomName}
              onChange={(e) => setRoomName(e.target.value)}
              maxLength={30}
              size="large"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">你的昵称</label>
            <Input
              placeholder="例如：张三"
              value={nickname}
              onChange={(e) => setNickname(e.target.value)}
              maxLength={20}
              size="large"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-3">房间有效期</label>
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
                    className={`w-full text-center py-3 rounded-lg border ${
                      ttl === option.value
                        ? 'bg-blue-600 border-blue-600 text-white'
                        : 'border-gray-300 hover:border-gray-400'
                    }`}
                  >
                    {option.label}
                  </Radio.Button>
                ))}
              </div>
            </Radio.Group>
            <p className="text-xs text-gray-500 mt-3 flex items-center gap-2">
              <Clock size={14} />
              有效期结束后，房间将自动销毁，所有数据将被永久删除
            </p>
          </div>
        </Space>
      </Modal>
    </div>
  );
}
