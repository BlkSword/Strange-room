'use client';

import { useState, useEffect, useRef } from 'react';

export default function App() {
    // 模拟聊天数据
    const [messages, setMessages] = useState([
        { id: 1, sender: 'Alice', content: '欢迎来到聊天室！', timestamp: new Date().toLocaleTimeString() },
        { id: 2, sender: 'Bob', content: '你好，大家！', timestamp: new Date().toLocaleTimeString() }
    ]);

    // 模拟在线用户
    const [users] = useState([
        { id: 1, name: 'Alice', online: true },
        { id: 2, name: 'Bob', online: true },
        { id: 3, name: 'Charlie', online: false }
    ]);

    const [inputValue, setInputValue] = useState('');
    const [activeUser] = useState('Alice');
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef(null);

    // 自动滚动到底部
    useEffect(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, [messages]);

    // 处理发送消息
    const handleSendMessage = () => {
        if (inputValue.trim() === '') return;

        const newMessage = {
            id: messages.length + 1,
            sender: activeUser,
            content: inputValue,
            timestamp: new Date().toLocaleTimeString()
        };

        setMessages([...messages, newMessage]);
        setInputValue('');
    };

    // 处理键盘事件
    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter') {
            handleSendMessage();
        }
    };

    return (
        <div className="flex h-screen bg-gray-900 text-white">
            {/* 用户列表 */}
            <div className="w-64 bg-gray-800 p-4 hidden md:block">
                <h2 className="text-xl font-bold mb-4">用户列表</h2>
                <ul>
                    {users.map(user => (
                        <li
                            key={user.id}
                            className={`flex items-center p-2 mb-2 rounded-lg ${user.name === activeUser ? 'bg-blue-600' : 'hover:bg-gray-700'
                                }`}
                        >
                            <span className={`w-3 h-3 rounded-full mr-2 ${user.online ? 'bg-green-500' : 'bg-gray-500'
                                }`}></span>
                            <span>{user.name}</span>
                        </li>
                    ))}
                </ul>
            </div>

            {/* 聊天主区域 */}
            <div className="flex-1 flex flex-col">
                {/* 聊天头部 */}
                <div className="p-4 border-b border-gray-700 flex justify-between items-center">
                    <div>
                        <h1 className="text-xl font-bold">群聊</h1>
                        <span className="text-sm text-gray-400">{users.filter(u => u.online).length}人在线</span>
                    </div>
                    <button className="md:hidden text-gray-400 hover:text-white">
                        <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                        </svg>
                    </button>
                </div>

                {/* 消息区域 */}
                <div className="flex-1 overflow-y-auto p-4 space-y-4">
                    {messages.map(message => (
                        <div
                            key={message.id}
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