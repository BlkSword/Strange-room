'use client';

import { useState, useEffect, useRef } from 'react';
import { Button, notification } from "antd";
import Peer from 'peerjs';

export default function ChatRoom() {
    const [peerInstance, setPeerInstance] = useState<Peer | null>(null);
    const [myUniqueId, setMyUniqueId] = useState<string>("");
    const [idToConnect, setIdToConnect] = useState('');
    const [connectedUsers, setConnectedUsers] = useState<{ id: string, conn: any }[]>([]);
    const [messages, setMessages] = useState<Array<{ sender: string, content: string, timestamp: string }>>([]);
    const [inputValue, setInputValue] = useState('');
    const [activeUser] = useState('Me');
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef(null);
    const [api, contextHolder] = notification.useNotification();

    const openNotification = (type: 'success' | 'info' | 'warning' | 'error', message: string, description?: string) => {
        api[type]({
            message,
            description,
            placement: 'topRight',
        });
    };

    useEffect(() => {
        const generateRandomString = () => Math.random().toString(36).substring(2);
        setMyUniqueId(generateRandomString());
    }, []);

    useEffect(() => {
        if (myUniqueId) {
            const peer = new Peer(myUniqueId, {
                host: 'localhost',
                port: 9000,
                path: '/talk',
            });

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
                    openNotification('success', '连接成功', `已与用户 ${conn.peer} 建立连接`);
                });
            });

            peer.on('error', (err) => {
                openNotification('error', '连接异常', err.message);
            });

            setPeerInstance(peer);

            return () => {
                peer.destroy();
            };
        }
    }, [myUniqueId]);

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
                openNotification('success', '连接成功', `已连接到用户 ${conn.peer}`);
            });
        }
    };

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

        connectedUsers.forEach(({ conn }) => {
            conn.send({ text: inputValue });
        });

        setInputValue('');
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter') {
            handleSendMessage();
        }
    };

    useEffect(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [messages]);

    return (
        <div className="flex h-screen bg-gray-900 text-white">
            {/* 用户列表 */}
            <div className="w-64 bg-gray-800 p-4 hidden md:block">
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
                    <button className="md:hidden text-gray-400 hover:text-white">
                        <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                        </svg>
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