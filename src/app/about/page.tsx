"use client";

import React from "react";
import {
  CheckCircle,
  Shield,
  Clock,
  Code,
  PenTool,
  Users,
  Server,
  Eye,
  Github,
  Rocket,
  Lock,
  Sparkles
} from "lucide-react";
import { Button } from "antd";
import Link from "next/link";

export default function AboutPage() {
  return (
    <div className="min-h-screen bg-sketch-background">
      {/* 顶部导航 */}
      <nav className="fixed top-0 left-0 right-0 z-50 bg-sketch-background/95 backdrop-blur-sm border-b-2 border-sketch-black">
        <div className="max-w-7xl mx-auto px-6 h-16 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-sketch bg-sketch-black flex items-center justify-center shadow-sketch">
              <Sparkles size={20} className="text-sketch-background" />
            </div>
            <span className="text-2xl font-bold font-marker text-sketch-black">
              Strange Room
            </span>
          </div>

          <div className="flex items-center gap-6">
            <Link href="/" className="text-base font-cave text-sketch-gray hover:text-sketch-black transition-colors">
              返回首页
            </Link>
            <a href="https://github.com/BlkSword/Strange-room" target="_blank" rel="noopener noreferrer" className="text-sketch-gray hover:text-sketch-black">
              <Github size={18} />
            </a>
          </div>
        </div>
      </nav>

      {/* Hero 区域 */}
      <section className="pt-32 pb-20 px-6">
        <div className="max-w-5xl mx-auto text-center">
          <div className="inline-flex items-center gap-2 px-6 py-3 rounded-sketch bg-sketch-light border-2 border-sketch-black text-sketch-black font-cave text-lg mb-8">
            <Sparkles size={16} />
            <span>端到端加密 · 自动销毁 · 无需注册</span>
          </div>

          <h1 className="text-6xl md:text-7xl font-marker mb-6 leading-tight hand-drawn-title">
            临时协作空间
            <br />
            <span className="text-sketch-accent">用完即焚</span>
          </h1>

          <p className="text-2xl text-sketch-gray mb-10 max-w-2xl mx-auto leading-relaxed font-cave">
            为黑客松、远程面试、临时讨论设计的协作空间。
            <br />
            白板、代码、聊天 —— 有效期结束后自动销毁。
          </p>

          <div className="flex items-center justify-center gap-4">
            <Link href="/">
              <Button
                type="primary"
                size="large"
                icon={<Rocket size={20} />}
                className="hand-drawn-btn text-xl px-10 py-4"
              >
                开始使用
              </Button>
            </Link>
          </div>

          {/* 信任指标 */}
          <div className="flex items-center justify-center gap-8 mt-16 pt-8 border-t-2 border-sketch-light">
            <div className="text-center">
              <div className="text-3xl font-bold text-sketch-black font-cave">0</div>
              <div className="text-base text-sketch-gray font-cave">服务器存储</div>
            </div>
            <div className="text-center">
              <div className="text-3xl font-bold text-sketch-black font-cave">100%</div>
              <div className="text-base text-sketch-gray font-cave">端到端加密</div>
            </div>
            <div className="text-center">
              <div className="text-3xl font-bold text-sketch-black font-cave">48h</div>
              <div className="text-base text-sketch-gray font-cave">最长有效期</div>
            </div>
          </div>
        </div>
      </section>

      {/* 核心功能 */}
      <section id="features" className="py-20 px-6 bg-sketch-card">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-4xl font-marker text-sketch-black mb-4 hand-drawn-title">三大核心功能</h2>
            <p className="text-sketch-gray text-xl font-cave">简单而强大的协作工具，无需学习即可上手</p>
          </div>

          <div className="grid md:grid-cols-3 gap-8">
            {[
              {
                icon: <PenTool size={36} className="text-sketch-accent" />,
                title: "协作白板",
                desc: "多人实时涂鸦，支持画笔、形状、橡皮擦。就像在真实白板上一样直观。",
                features: ["自由绘制", "多种画笔工具", "多人协同", "导出图片"]
              },
              {
                icon: <Code size={36} className="text-sketch-black" />,
                title: "代码协同",
                desc: "内置 Monaco 编辑器，支持 JavaScript/Python/Go 等多种语言，实时协同编程。",
                features: ["语法高亮", "自动补全", "多人编辑", "光标追踪"]
              },
              {
                icon: <Clock size={36} className="text-sketch-accent" />,
                title: "自动销毁",
                desc: "1/6/24/48小时可选，倒计时结束后所有数据永久删除，不留任何痕迹。",
                features: ["定时销毁", "手动删除", "数据清除", "隐私保护"]
              }
            ].map((feature, i) => (
              <div
                key={i}
                className="mystic-card hand-drawn-border hover:translate-y-[-4px] transition-transform duration-200"
              >
                <div className="w-16 h-16 rounded-sketch bg-sketch-light flex items-center justify-center mb-6 border-2 border-sketch-black">
                  {feature.icon}
                </div>
                <h3 className="text-2xl font-semibold text-sketch-black mb-3 font-cave">{feature.title}</h3>
                <p className="text-sketch-gray leading-relaxed mb-4 font-cave">{feature.desc}</p>
                <ul className="space-y-2">
                  {feature.features.map((f, j) => (
                    <li key={j} className="flex items-center gap-2 text-sm text-sketch-gray font-cave">
                      <CheckCircle size={16} className="text-sketch-accent" />
                      {f}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* 技术架构 */}
      <section id="architecture" className="py-20 px-6">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-4xl font-marker text-sketch-black mb-4 hand-drawn-title">技术架构</h2>
            <p className="text-sketch-gray text-xl font-cave">基于 WebRTC 的纯 P2P 架构，数据不经过服务器</p>
          </div>

          <div className="grid md:grid-cols-3 gap-8">
            {[
              {
                icon: <Server size={32} className="text-sketch-black" />,
                title: "信令服务器",
                desc: "仅负责建立 WebRTC 连接，不存储任何业务数据。连接建立后即退出，无法窥探通信内容。",
                tech: ["Node.js", "WebSocket", "PeerJS"]
              },
              {
                icon: <Users size={32} className="text-sketch-accent" />,
                title: "P2P 通信",
                desc: "数据直接在用户设备间传输，通过 WebRTC 建立加密通道，确保通信安全和低延迟。",
                tech: ["WebRTC", "DataChannel", "SRTP"]
              },
              {
                icon: <Eye size={32} className="text-sketch-black" />,
                title: "本地存储",
                desc: "房间数据存储在浏览器 IndexedDB 中，本地加密，房间销毁后自动清除。",
                tech: ["IndexedDB", "Yjs", "CRDT"]
              }
            ].map((item, i) => (
              <div key={i} className="mystic-card hand-drawn-border">
                <div className="flex items-center gap-4 mb-4">
                  <div className="w-14 h-14 rounded-sketch bg-sketch-light flex items-center justify-center border-2 border-sketch-black">
                    {item.icon}
                  </div>
                  <h3 className="text-xl font-semibold text-sketch-black font-cave">{item.title}</h3>
                </div>
                <p className="text-sketch-gray mb-4 font-cave">{item.desc}</p>
                <div className="flex flex-wrap gap-2">
                  {item.tech.map((t, j) => (
                    <span key={j} className="px-4 py-2 bg-sketch-light rounded-sketch text-sm text-sketch-black border-2 border-sketch-black font-cave">
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
      <section id="security" className="py-20 px-6 bg-sketch-card">
        <div className="max-w-6xl mx-auto">
          <div className="grid md:grid-cols-2 gap-12 items-center">
            <div>
              <h2 className="text-4xl font-marker text-sketch-black mb-6">
                隐私优先
                <br />
                <span className="text-sketch-accent">端到端加密</span>
              </h2>
              <p className="text-xl text-sketch-gray mb-8 leading-relaxed font-cave">
                所有数据通过 WebRTC 点对点传输，直接在你的设备之间流转。
                我们的服务器仅负责信令交换，无法窥探任何内容。
              </p>

              <div className="space-y-4">
                {[
                  { icon: <Shield size={24} />, title: "P2P 加密传输", desc: "数据不经过服务器存储" },
                  { icon: <Clock size={24} />, title: "自动销毁机制", desc: "到期后永久删除所有记录" },
                  { icon: <Users size={24} />, title: "无需注册登录", desc: "打开即用，不收集任何个人信息" }
                ].map((item, i) => (
                  <div key={i} className="flex items-start gap-4">
                    <div className="flex-shrink-0 w-12 h-12 rounded-sketch bg-sketch-light flex items-center justify-center text-sketch-black border-2 border-sketch-black">
                      {item.icon}
                    </div>
                    <div>
                      <h4 className="font-semibold text-sketch-black mb-1 font-cave text-lg">{item.title}</h4>
                      <p className="text-sm text-sketch-gray font-cave">{item.desc}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            <div className="mystic-card hand-drawn-border">
              <div className="space-y-6">
                <div className="flex items-center gap-3">
                  <Lock size={28} className="text-sketch-black" />
                  <span className="font-semibold text-sketch-black font-cave text-xl">安全架构</span>
                </div>
                <div className="space-y-4 text-sketch-gray">
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5 text-sketch-accent" />
                    <span className="font-cave">WebRTC 点对点连接，数据直接在用户设备间传输</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5 text-sketch-accent" />
                    <span className="font-cave">信令服务器仅交换连接信息，不存储任何业务数据</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5 text-sketch-accent" />
                    <span className="font-cave">房间数据存储在浏览器 IndexedDB，本地加密</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5 text-sketch-accent" />
                    <span className="font-cave">倒计时结束自动清理，无法恢复</span>
                  </div>
                  <div className="flex items-start gap-3">
                    <CheckCircle size={20} className="flex-shrink-0 mt-0.5 text-sketch-accent" />
                    <span className="font-cave">开源代码，可自行审查和部署</span>
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
            <h2 className="text-4xl font-marker text-sketch-black mb-4 hand-drawn-title">适用场景</h2>
            <p className="text-sketch-gray text-xl font-cave">为临时协作而设计，满足各种即时沟通需求</p>
          </div>

          <div className="grid md:grid-cols-2 gap-6">
            {[
              { icon: <Users size={32} />, title: "黑客松协作", desc: "48小时内快速组队，在白板上讨论方案，共享代码片段", tags: ["组队", "讨论", "共享"] },
              { icon: <Code size={32} />, title: "技术面试", desc: "在线白板讲解架构图，Monaco 编辑器演示代码实现", tags: ["面试", "白板", "代码"] },
              { icon: <PenTool size={32} />, title: "临时讨论", desc: "不想留下聊天记录？用完即焚的讨论空间，隐私无忧", tags: ["隐私", "临时", "无痕"] },
              { icon: <Shield size={32} />, title: "敏感信息交换", desc: "端到端加密传输，适合交换 API 密钥、配置等敏感信息", tags: ["加密", "安全", "P2P"] }
            ].map((useCase, i) => (
              <div
                key={i}
                className="mystic-card hand-drawn-border hover:translate-y-[-4px] transition-transform duration-200 cursor-pointer"
              >
                <div className="flex items-start gap-4">
                  <div className="flex-shrink-0 w-14 h-14 rounded-sketch bg-sketch-light flex items-center justify-center text-sketch-black border-2 border-sketch-black">
                    {useCase.icon}
                  </div>
                  <div className="flex-1">
                    <h3 className="text-xl font-semibold text-sketch-black mb-2 font-cave">{useCase.title}</h3>
                    <p className="text-sketch-gray text-sm mb-4 font-cave">{useCase.desc}</p>
                    <div className="flex items-center gap-2">
                      {useCase.tags.map((tag, j) => (
                        <span key={j} className="px-3 py-1 bg-sketch-light text-sketch-black text-sm rounded-sketch border-2 border-sketch-black font-cave">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* 开源信息 */}
      <section className="py-20 px-6 bg-sketch-card">
        <div className="max-w-4xl mx-auto text-center">
          <div className="inline-flex items-center gap-3 px-6 py-3 rounded-sketch bg-sketch-light border-2 border-sketch-black mb-8">
            <Github size={24} className="text-sketch-black" />
            <span className="text-sketch-black font-cave text-lg">开源项目</span>
          </div>

          <h2 className="text-4xl font-marker text-sketch-black mb-4 hand-drawn-title">代码完全开源</h2>
          <p className="text-sketch-gray mb-8 max-w-2xl mx-auto text-lg font-cave">
            Strange Room 是一个开源项目，你可以自由查看、修改和部署代码。
            所有代码都在 GitHub 上公开，欢迎贡献和反馈。
          </p>

          <div className="flex items-center justify-center gap-8 mb-8">
            <div className="text-center">
              <div className="text-3xl font-bold text-sketch-black font-cave">Next.js</div>
              <div className="text-base text-sketch-gray font-cave">前端框架</div>
            </div>
            <div className="text-center">
              <div className="text-3xl font-bold text-sketch-black font-cave">WebRTC</div>
              <div className="text-base text-sketch-gray font-cave">P2P 通信</div>
            </div>
            <div className="text-center">
              <div className="text-3xl font-bold text-sketch-black font-cave">Yjs</div>
              <div className="text-base text-sketch-gray font-cave">协同编辑</div>
            </div>
          </div>

          <Button
            type="primary"
            size="large"
            icon={<Github size={20} />}
            onClick={() => window.open('https://github.com/BlkSword/Strange-room', '_blank')}
            className="hand-drawn-btn text-xl px-10 py-4"
          >
            查看 GitHub 仓库
          </Button>
        </div>
      </section>

      {/* 页脚 */}
      <footer className="py-12 px-6 border-t-2 border-sketch-light">
        <div className="max-w-6xl mx-auto">
          <div className="flex flex-col md:flex-row items-center justify-between gap-6">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-sketch bg-sketch-black flex items-center justify-center shadow-sketch">
                <Sparkles size={20} className="text-sketch-background" />
              </div>
              <span className="font-semibold text-sketch-black font-cave text-lg">Strange Room</span>
            </div>

            <div className="flex items-center gap-6 text-base text-sketch-gray font-cave">
              <Link href="/" className="hover:text-sketch-black">
                返回首页
              </Link>
              <a href="https://github.com/BlkSword/Strange-room" target="_blank" rel="noopener noreferrer" className="hover:text-sketch-black">
                GitHub
              </a>
            </div>

            <div className="text-base text-sketch-gray font-cave">
              遵循 Apache-2.0 开源协议
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
