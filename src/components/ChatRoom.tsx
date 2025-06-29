'use client';

import { useState, useEffect, useRef } from 'react';
import { Button, notification } from "antd";
import { MenuUnfoldOutlined } from '@ant-design/icons';
import Peer from 'peerjs';

export default function ChatRoom() {
    // 状态管理
    const [peerInstance, setPeerInstance] = useState<Peer | null>(null); // Peer实例
    const [myUniqueId, setMyUniqueId] = useState<string>(""); // 当前用户唯一ID
    const [idToConnect, setIdToConnect] = useState(''); // 待连接的用户ID
    const [connectedUsers, setConnectedUsers] = useState<{ id: string, conn: any }[]>([]); // 已连接用户列表
    const [messages, setMessages] = useState<Array<{ sender: string, content: string, timestamp: string }>>([]); // 消息历史
    const [inputValue, setInputValue] = useState(''); // 输入框内容
    const [activeUser] = useState('Me'); // 当前活跃用户
    // 消息容器引用
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef(null);
    // 通知API和上下文
    const [api, contextHolder] = notification.useNotification();

    // 菜单状态管理
    const [isMenuOpen, setIsMenuOpen] = useState(false);
    const menuRef = useRef<HTMLDivElement>(null);

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


    const openNotification = (type: 'success' | 'info' | 'warning' | 'error', message: string, description?: string) => {
        api[type]({
            message,
            description,
            placement: 'topRight',
        });
    };

    // 组件挂载时生成随机用户ID
    useEffect(() => {
        const generateRandomString = () => Math.random().toString(36).substring(2);
        setMyUniqueId(generateRandomString());
    }, []);

    // 初始化Peer实例
    useEffect(() => {
        if (myUniqueId) {
            // 创建Peer实例并配置连接参数
            const peer = new Peer(myUniqueId, {
                host: 'localhost',
                port: 9000,
                path: '/talk',
            });

            // 监听连接事件
            peer.on('connection', (conn) => {
                // 监听数据接收事件
                conn.on('data', (data: any) => {
                    setMessages(prev => [...prev, {
                        sender: conn.peer,
                        content: data.text,
                        timestamp: new Date().toLocaleTimeString()
                    }]);
                });

                // 连接建立成功
                conn.on('open', () => {
                    setConnectedUsers(prev => [...prev, { id: conn.peer, conn }]);
                    setMessages(prev => [...prev, {
                        sender: 'System',
                        content: `${conn.peer} 已连接`,
                        timestamp: new Date().toLocaleTimeString()
                    }]);
                    openNotification('success', '连接成功', `已与用户 ${conn.peer} 建立连接`);
                });
            });

            // 监听错误事件
            peer.on('error', (err) => {
                openNotification('error', '连接异常', err.message);
            });

            setPeerInstance(peer);

            // 清理函数
            return () => {
                peer.destroy();
            };
        }
    }, [myUniqueId]);

    // 处理连接按钮点击事件
    const handleConnect = () => {
        if (!idToConnect.trim()) {
            openNotification('warning', '请输入有效ID', '连接ID不能为空');
            return;
        }

        if (idToConnect === myUniqueId) {
            openNotification('warning', '无法连接自己', '不能使用自己的ID进行连接');
            return;
        }

        if (connectedUsers.find(u => u.id === idToConnect)) {
            openNotification('warning', '已存在的连接', `用户ID: ${idToConnect}`);
            return;
        }

        // 建立新连接
        const conn = peerInstance?.connect(idToConnect);

        if (conn) {
            // 监听数据接收
            conn.on('data', (data: any) => {
                setMessages(prev => [...prev, {
                    sender: conn.peer,
                    content: data.text,
                    timestamp: new Date().toLocaleTimeString()
                }]);
            });

            // 连接成功回调
            conn.on('open', () => {
                setConnectedUsers(prev => [...prev, { id: conn.peer, conn }]);
                setMessages(prev => [...prev, {
                    sender: 'System',
                    content: `已连接到 ${conn.peer}`,
                    timestamp: new Date().toLocaleTimeString()
                }]);
                openNotification('success', '连接成功', `已连接到用户 ${conn.peer}`);
            });
        }
    };

    // 发送消息处理函数
    const handleSendMessage = () => {
        if (inputValue.trim() === '') {
            openNotification('warning', '请输入消息内容');
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

    return (
        <div className="flex h-screen bg-gray-900 text-white">
            {/* 用户列表 */}
            <div
                className={`fixed inset-y-0 left-0 z-50 w-64 bg-gray-800 p-4 transform ${isMenuOpen ? 'translate-x-0' : '-translate-x-full'} transition-transform duration-300 ease-in-out md:relative md:translate-x-0 md:flex md:flex-col md:h-full`}
                ref={menuRef}
            >
                <h2 className="text-xl font-bold mb-4">我的ID</h2>
                <p className="mb-4 text-sm text-gray-300">{myUniqueId}</p>

                {contextHolder}

                <div className="mt-4">
                    <h3 className="text-lg font-semibold mb-2">连接用户</h3>
                    <div className="flex space-x-1">
                        <input
                            type="text"
                            value={idToConnect}
                            onChange={(e) => setIdToConnect(e.target.value)}
                            placeholder="输入ID"
                            className="flex-1 bg-gray-700 text-white rounded px-2 py-1 text-sm"
                        />
                    </div>
                    <Button
                        type="primary"
                        onClick={handleConnect}
                        disabled={!idToConnect}
                        className="mt-2"
                    >
                        连接
                    </Button>
                </div>

                <div className="mt-6">
                    <h3 className="text-lg font-semibold mb-2">在线用户</h3>
                    <ul>
                        {connectedUsers.map((user, index) => (
                            <li
                                key={index}
                                className={`flex items-center p-2 mb-2 rounded-lg ${user.id === myUniqueId ? 'bg-blue-600' : 'hover:bg-gray-700'
                                    }`}
                            >
                                <span className="w-3 h-3 rounded-full mr-2 bg-green-500"></span>
                                <span>{user.id}</span>
                            </li>
                        ))}
                    </ul>
                </div>
            </div>

            {/* 聊天主区域 */}
            <div className="flex-1 flex flex-col">
                {/* 聊天头部 */}
                <div className="p-4 border-b border-gray-700 flex justify-between items-center">
                    <div>
                        <h1 className="text-xl font-bold">群聊</h1>
                        <span className="text-sm text-gray-400">{connectedUsers.length + 1}人在线</span>
                    </div>
                    <button
                        className="md:hidden text-gray-400 hover:text-white"
                        onClick={() => setIsMenuOpen(!isMenuOpen)}
                    >
                        <MenuUnfoldOutlined className="w-6 h-6" />
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
                <div className="p-4 border-t border-gray-700">
                    <div className="flex items-center space-x-2">
                        <input
                            ref={inputRef}
                            type="text"
                            value={inputValue}
                            onChange={(e) => setInputValue(e.target.value)}
                            onKeyDown={handleKeyDown}
                            placeholder="输入消息..."
                            className="flex-1 bg-gray-800 text-white rounded-full px-4 py-2 focus:outline-none focus:ring-2 focus:ring-blue-500"
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