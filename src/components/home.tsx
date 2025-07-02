"use client";

import React from "react";
import { Button } from "antd";
import { MessageOutlined, RocketOutlined, SmileOutlined, CheckCircleOutlined, CloudOutlined, LockOutlined } from "@ant-design/icons";
import { useRouter } from "next/navigation";

const features = [
    {
        icon: <MessageOutlined className="text-4xl text-blue-500" />,
        title: "匿名畅聊",
        desc: "无需注册，输入ID即可与他人建立点对点加密聊天。"
    },
    {
        icon: <RocketOutlined className="text-4xl text-purple-500" />,
        title: "极速连接",
        desc: "基于WebRTC，消息实时传递，体验流畅。"
    },
    {
        icon: <SmileOutlined className="text-4xl text-yellow-500" />,
        title: "极简界面",
        desc: "现代毛玻璃风格，支持移动端，操作简单易用。"
    }
];

const advantages = [
    {
        icon: <CheckCircleOutlined className="text-2xl text-green-400" />,
        title: "开源免费",
        desc: "完全开源，永久免费使用，代码透明可查。"
    },
    {
        icon: <LockOutlined className="text-2xl text-blue-400" />,
        title: "端到端加密",
        desc: "所有消息点对点加密，保障你的隐私安全。"
    },
    {
        icon: <CloudOutlined className="text-2xl text-purple-400" />,
        title: "多平台支持",
        desc: "支持主流浏览器与移动端，随时随地畅聊。"
    }
];

export default function Home() {
    const router = useRouter();

    return (
        <div className="relative min-h-screen flex flex-col items-center justify-start bg-gradient-to-br from-blue-900 via-black to-purple-900 text-white overflow-x-hidden">
            <div className="w-full flex flex-col items-center justify-center pt-32 pb-20 relative z-10">
                <div className="backdrop-blur-2xl bg-black/40 rounded-3xl px-10 py-12 shadow-2xl max-w-3xl mx-auto flex flex-col items-center">
                    <div className="flex items-center gap-4 mb-4">
                        <img src="/favicon.ico" alt="logo" className="w-12 h-12 rounded-xl shadow-lg" />
                    </div>
                    <h1 className="text-5xl font-bold mb-4 text-center tracking-tight drop-shadow-lg">Strange Room</h1>
                    <p className="text-xl text-gray-300 mb-8 text-center max-w-xl">一个基于WebRTC的极简匿名聊天室，点对点加密，畅所欲言。安全、自由、无痕。</p>
                    <Button
                        type="primary"
                        size="large"
                        className="px-10 py-3 text-xl bg-blue-600 hover:bg-blue-700 rounded-full shadow-md transition-all duration-200"
                        onClick={() => router.push('/chat')}
                    >
                        立即进入聊天室
                    </Button>
                </div>
            </div>

            {/* 毛玻璃渐变背景 */}
            <div className="fixed inset-0 z-0 bg-cover bg-center blur-3xl scale-110 opacity-70 pointer-events-none" style={{
                backgroundImage: 'linear-gradient(120deg, #1e3a8a 0%, #000 60%, #6d28d9 100%)',
                filter: 'blur(100px)',
                transform: 'scale(1.2)'
            }}></div>

            <section className="relative z-10 w-full max-w-5xl mx-auto px-4 py-12 mt-2">
                <h2 className="text-3xl font-bold text-center mb-10 tracking-tight">核心特性</h2>
                <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
                    {features.map((f, i) => (
                        <div
                            key={i}
                            className="flex flex-col items-center p-8 bg-gradient-to-br from-gray-800/80 to-gray-900/80 rounded-2xl shadow-lg transition-transform duration-300 ease-out hover:scale-105 hover:shadow-2xl hover:bg-gray-800/90 group cursor-pointer"
                            style={{ minHeight: 220 }}
                        >
                            <div className="mb-3 group-hover:animate-bounce">
                                {f.icon}
                            </div>
                            <div className="font-bold text-xl mt-2 mb-2 tracking-wide group-hover:text-blue-400 transition-colors duration-200">{f.title}</div>
                            <div className="text-gray-400 text-base text-center leading-relaxed">{f.desc}</div>
                        </div>
                    ))}
                </div>
            </section>

            {/* Why Choose Us Section */}
            <section className="relative z-10 w-full max-w-5xl mx-auto px-4 py-12">
                <h2 className="text-3xl font-bold text-center mb-10 tracking-tight">为什么选择 Strange Room</h2>
                <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
                    {advantages.map((a, i) => (
                        <div key={i} className="flex flex-col items-center p-8 bg-black/60 rounded-2xl shadow-lg">
                            <div className="mb-3">{a.icon}</div>
                            <div className="font-bold text-lg mb-2">{a.title}</div>
                            <div className="text-gray-400 text-base text-center leading-relaxed">{a.desc}</div>
                        </div>
                    ))}
                </div>
            </section>

            {/* Footer */}
            <footer className="relative z-10 w-full text-center text-gray-500 py-8 text-sm">
                <div>© {new Date().getFullYear()} Strange Room. Inspired by <a href="https://github.com/BlkSword/Strange-room" className="underline hover:text-blue-400" target="_blank" rel="noopener noreferrer">BlkSword</a>.</div>
            </footer>
        </div>
    );
}
