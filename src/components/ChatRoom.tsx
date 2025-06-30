// 优化：
// √修改服务端，使其可以兼容域名访问
// 加一个主页面，进行简单的介绍————介绍是否真的隐私保密
// 聊天界面可以为玻璃透明，并且加一点气泡动画。示例：https://codepen.io/supah/pen/jqOBqp

'use client';


import React, { useState, useEffect, useRef } from 'react';
import { Button, message, Space } from 'antd';
import { MenuOutlined, CopyOutlined, SyncOutlined } from '@ant-design/icons';
import Peer from 'peerjs';

// 毛玻璃效果样式
const GlassEffect = () => (
    <div className="fixed inset-0 z-0 bg-cover bg-center blur-3xl transform scale-105" style={{
        backgroundImage: 'url(https://images.unsplash.com/photo-1451186859696-371d9477be93?ixlib=rb-4.0.3&auto=format&fit=crop&w=1920&q=80)',
        filter: 'blur(80px)',
        transform: 'scale(1.2)'
    }}></div>
);

export default function ChatRoom() {
    // 存储Peer实例，用于管理WebRTC连接
    const [peerInstance, setPeerInstance] = useState<Peer | null>(null);
    // 当前用户的唯一标识符
    const [myUniqueId, setMyUniqueId] = useState<string>("");
    // 待连接的远程用户ID
    const [idToConnect, setIdToConnect] = useState('');
    // 已连接用户列表，包含用户ID和连接对象
    const [connectedUsers, setConnectedUsers] = useState<{ id: string, conn: any }[]>([]);
    // 聊天消息列表，包含发送者、内容、时间戳和加载状态
    const [messages, setMessages] = useState<Array<{ sender: string, content: string, timestamp: string, loading?: boolean }>>([]);
    // 输入框内容状态
    const [inputValue, setInputValue] = useState('');
    // 当前活跃用户标识（始终为'Me'）
    const [activeUser] = useState('Me');
    // 控制侧边栏菜单的展开/收起状态
    const [isMenuOpen, setIsMenuOpen] = useState(false);
    // 消息列表底部参考点
    const messagesEndRef = useRef<HTMLDivElement>(null);
    // 输入框参考点
    const inputRef = useRef(null);
    // 使用 message API
    const [messageApi, contextHolder] = message.useMessage();
    // 侧边栏菜单参考点
    const menuRef = useRef<HTMLDivElement>(null);
    // 复制成功提示状态
    const [isCopied, setIsCopied] = useState(false);

    // 通知函数
    const openMessage = (type: 'success' | 'error' | 'warning', content: string) => {
        messageApi.open({
            type,
            content,
            duration: 2,
        });
    };

    // 外部点击检测关闭菜单
    useEffect(() => {
        const handleClickOutside = (event: MouseEvent | TouchEvent) => {
            if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
                setIsMenuOpen(false);
            }
        };
        document.addEventListener('mousedown', handleClickOutside);
        document.addEventListener('touchstart', handleClickOutside);
        return () => {
            document.removeEventListener('mousedown', handleClickOutside);
            document.removeEventListener('touchstart', handleClickOutside);
        };
    }, []);

    // 生成随机ID
    useEffect(() => {
        const generateRandomString = () => Math.random().toString(36).substring(2);
        setMyUniqueId(generateRandomString());
    }, []);

    // 初始化Peer实例
    useEffect(() => {
        if (myUniqueId) {
            const peer = new Peer(myUniqueId, {
                host: 'localhost',
                port: 9000,
                path: '/talk',
            });

            // 监听连接事件
            peer.on('connection', (conn) => {
                conn.on('data', (data: any) => {
                    setMessages(prev => [...prev, {
                        sender: conn.peer,
                        content: data.text,
                        timestamp: new Date().toLocaleTimeString()
                    }]);
                });

                conn.on('open', () => {
                    setConnectedUsers(prev => [...prev, { id: conn.peer, conn }]);
                    setMessages(prev => [...prev, {
                        sender: 'System',
                        content: `${conn.peer} 已连接`,
                        timestamp: new Date().toLocaleTimeString()
                    }]);
                    openMessage('success', `已与用户 ${conn.peer} 建立连接`);
                });
            });

            // 监听错误事件
            peer.on('error', (err) => {
                openMessage('error', err.message);
            });

            setPeerInstance(peer);
            return () => {
                peer.destroy();
            };
        }
    }, [myUniqueId]);

    // 处理连接按钮点击事件
    const handleConnect = () => {
        if (!idToConnect.trim()) {
            openMessage('warning', '连接ID不能为空');
            return;
        }

        if (idToConnect === myUniqueId) {
            openMessage('warning', '不能使用自己的ID进行连接');
            return;
        }

        if (connectedUsers.find(u => u.id === idToConnect)) {
            openMessage('warning', `用户ID: ${idToConnect}`);
            return;
        }

        const conn = peerInstance?.connect(idToConnect);
        if (conn) {
            conn.on('data', (data: any) => {
                setMessages(prev => [...prev, {
                    sender: conn.peer,
                    content: data.text,
                    timestamp: new Date().toLocaleTimeString()
                }]);
            });

            conn.on('open', () => {
                setConnectedUsers(prev => [...prev, { id: conn.peer, conn }]);
                setMessages(prev => [...prev, {
                    sender: 'System',
                    content: `已连接到 ${conn.peer}`,
                    timestamp: new Date().toLocaleTimeString()
                }]);
                openMessage('success', `已连接到用户 ${conn.peer}`);
            });
        }
    };

    // 发送消息处理函数
    const handleSendMessage = () => {
        if (inputValue.trim() === '') {
            openMessage('warning', '请输入消息内容');
            return;
        }

        const timestamp = new Date().toLocaleTimeString();
        const newMessage = {
            sender: activeUser,
            content: inputValue,
            timestamp
        };

        setMessages(prev => [...prev, newMessage]);

        // 向所有连接的用户广播消息
        connectedUsers.forEach(({ conn }) => {
            conn.send({ text: inputValue });
        });

        setInputValue('');
    };

    // 输入框回车事件处理
    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter') {
            handleSendMessage();
        }
    };

    // 消息滚动到底部效果
    useEffect(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [messages]);

    // 复制ID到剪贴板
    const handleCopy = async () => {
        try {
            await navigator.clipboard.writeText(myUniqueId);
            setIsCopied(true);
            openMessage('success', '已复制到剪贴板');
            setTimeout(() => setIsCopied(false), 2000);
        } catch (err) {
            console.error('复制失败:', err);
            openMessage('error', '复制失败');
        }
    };

    return (
        <div className="relative flex h-screen bg-gray-900 text-white overflow-hidden">
            {/* 毛玻璃背景 */}
            <GlassEffect />
            {/* 用户列表 */}
            <div
                className={`fixed inset-y-0 right-0 z-50 w-64 bg-gray-800 bg-opacity-50 backdrop-blur-lg p-4 transform ${isMenuOpen ? 'translate-x-0' : 'translate-x-full'} transition-transform duration-300 ease-in-out md:relative md:translate-x-0 md:flex md:flex-col md:h-full border-l border-white border-opacity-10`}
                ref={menuRef}
            >
                <h2 className="text-xl font-bold mb-4">我的ID</h2>
                <div className="border p-2 rounded-lg mb-4">
                    <div className="flex justify-between items-center">
                        <p className="text-sm text-gray-300">{myUniqueId}</p>
                        <button
                            onClick={handleCopy}
                            className="ml-2 text-gray-400 hover:text-white transition-colors"
                            aria-label="复制ID"
                        >
                            <CopyOutlined />
                        </button>
                    </div>
                </div>
                {contextHolder}
                <div className="mt-4">
                    <h3 className="text-lg font-semibold mb-2">连接用户</h3>
                    <div className="flex space-x-1">
                        <input
                            type="text"
                            value={idToConnect}
                            onChange={(e) => setIdToConnect(e.target.value)}
                            placeholder="输入ID"
                            className="flex-1 bg-gray-700 bg-opacity-50 backdrop-blur-sm text-white rounded px-2 py-1 text-sm"
                        />
                    </div>
                    <div className="flex justify-end">
                        <Button
                            type="primary"
                            onClick={handleConnect}
                            disabled={!idToConnect}
                            className="mt-2 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                            连接
                        </Button>
                    </div>
                </div>
                <div className="mt-6">
                    <h3 className="text-lg font-semibold mb-2">在线用户</h3>
                    <ul>
                        {connectedUsers.map((user, index) => (
                            <li
                                key={index}
                                className={`flex items-center p-2 mb-2 rounded-lg ${user.id === myUniqueId ? 'bg-blue-600' : 'hover:bg-gray-700'} bg-opacity-50 backdrop-blur-sm`}
                            >
                                <span className="w-3 h-3 rounded-full mr-2 bg-green-500"></span>
                                <span>{user.id}</span>
                            </li>
                        ))}
                    </ul>
                </div>
            </div>
            {/* 聊天主区域 */}
            <div className="flex-1 flex flex-col relative z-10">
                {/* 聊天头部 */}
                <div className="p-4 border-b border-gray-700 flex justify-between items-center backdrop-blur-sm bg-black bg-opacity-30">
                    <div>
                        <h1 className="text-xl font-bold">匿名聊天室</h1>
                        <span className="text-sm text-gray-400">{connectedUsers.length + 1}人在线</span>
                    </div>
                    <button
                        className="md:hidden text-gray-400 hover:text-white"
                        onClick={() => setIsMenuOpen(!isMenuOpen)}
                    >
                        <MenuOutlined className="w-6 h-6" />
                    </button>
                </div>
                {/* 消息区域 */}
                <div className="flex-1 overflow-y-auto p-4 space-y-4">
                    {messages.map((message, index) => (
                        <div
                            key={index}
                            className={`flex ${message.sender === activeUser ? 'justify-end' : 'justify-start'}`}
                        >
                            <div className={`max-w-xs md:max-w-md rounded-lg p-3 ${message.sender === activeUser
                                ? 'bg-blue-600 text-white rounded-br-none'
                                : 'bg-gray-700 text-white rounded-bl-none'
                                }`}>
                                <div className="font-bold text-sm mb-1">{message.sender}</div>
                                <div className="break-words">{message.content}</div>
                                <div className="text-xs text-gray-300 mt-1 text-right">{message.timestamp}</div>
                            </div>
                        </div>
                    ))}
                    <div ref={messagesEndRef} />
                </div>
                {/* 输入区域 */}
                <div className="p-4 border-t border-gray-700 backdrop-blur-sm bg-black bg-opacity-30">
                    <div className="flex items-center space-x-2">
                        <input
                            ref={inputRef}
                            type="text"
                            value={inputValue}
                            onChange={(e) => setInputValue(e.target.value)}
                            onKeyDown={handleKeyDown}
                            placeholder="输入消息..."
                            className="flex-1 bg-gray-800 bg-opacity-50 backdrop-blur-sm text-white rounded-full px-4 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
                        />
                        <button
                            onClick={handleSendMessage}
                            disabled={!inputValue.trim()}
                            className={`p-2 rounded-full ${inputValue.trim()
                                ? 'bg-blue-600 hover:bg-blue-700'
                                : 'bg-gray-700 cursor-not-allowed'
                                }`}
                        >
                            <svg className="w-6 h-6 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
                            </svg>
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}