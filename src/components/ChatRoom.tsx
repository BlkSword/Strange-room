'use client';


import React, { useState, useEffect, useRef } from 'react';
import { Button, message, Space, Modal } from 'antd';
import { MenuOutlined, CopyOutlined, SyncOutlined, UploadOutlined, AudioOutlined, AudioMutedOutlined, VideoCameraOutlined, VideoCameraAddOutlined, FullscreenOutlined, FullscreenExitOutlined, CloseOutlined } from '@ant-design/icons';
import Peer from 'peerjs';
import "../css/messages.css"

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
    // 聊天消息列表，包含发送者、内容、时间戳、类型和加载状态
    const [messages, setMessages] = useState<Array<{ sender: string, content: string, timestamp: string, type?: 'text' | 'image' | 'video', loading?: boolean }>>([]);
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
    // 聊天输入区文件选择
    const fileInputRef = useRef<HTMLInputElement>(null);
    // 图片预览相关 state
    const [previewImage, setPreviewImage] = useState<string | null>(null);
    // 视频通话相关 state
    const [localStream, setLocalStream] = useState<MediaStream | null>(null);
    const [remoteStreams, setRemoteStreams] = useState<{ id: string, stream: MediaStream }[]>([]);
    const [isCalling, setIsCalling] = useState(false);
    const [isMicOn, setIsMicOn] = useState(true);
    const [isCamOn, setIsCamOn] = useState(true);
    const [callRequest, setCallRequest] = useState<{ call: any, visible: boolean } | null>(null);
    const [isFullscreen, setIsFullscreen] = useState(false);
    const mediaConnections = useRef<{ [id: string]: any }>({});
    // 视频元素 ref
    const localVideoRef = useRef<HTMLVideoElement | null>(null);
    const remoteVideoRefs = useRef<{ [id: string]: HTMLVideoElement | null }>({});
    // 全屏切换
    const videoBarRef = useRef<HTMLDivElement | null>(null);

    // 进入聊天室欢迎消息
    useEffect(() => {
        setMessages(prev => {
            if (prev.length === 0) {
                return [
                    {
                        sender: 'System',
                        content: '欢迎来到匿名聊天室！',
                        timestamp: new Date().toLocaleTimeString(),
                        type: 'text' as const
                    }
                ];
            }
            return prev;
        });
    }, []);

    // 通知函数
    const openMessage = (type: 'success' | 'error' | 'warning' | 'info', content: string) => {
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
                host: 'talk.blksword.com',
                port: 9000,
                path: '/talk',
                secure: true,
            });

            // 监听连接事件
            peer.on('connection', (conn) => {
                conn.on('data', (data: any) => {
                    if (data.type === 'image' || data.type === 'video') {
                        setMessages(prev => [...prev, {
                            sender: conn.peer,
                            content: data.data,
                            timestamp: new Date().toLocaleTimeString(),
                            type: data.type as 'image' | 'video'
                        }]);
                    } else {
                        setMessages(prev => [...prev, {
                            sender: conn.peer,
                            content: data.text,
                            timestamp: new Date().toLocaleTimeString(),
                            type: 'text' as const
                        }]);
                    }
                });

                conn.on('open', () => {
                    setConnectedUsers(prev => [...prev, { id: conn.peer, conn }]);
                    setMessages(prev => [...prev, {
                        sender: 'System',
                        content: `${conn.peer} 已连接`,
                        timestamp: new Date().toLocaleTimeString(),
                        type: 'text' as const
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
    // 刷新ID函数
    const handleRefresh = () => {
        const newId = Math.random().toString(36).substring(2);
        setMyUniqueId(newId);
        openMessage('success', 'ID已刷新');
    };

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

        // 显示连接中通知
        openMessage('info', '正在建立连接...');

        const conn = peerInstance?.connect(idToConnect);
        if (conn) {
            // 设置连接超时处理
            const connectionTimeout = setTimeout(() => {
                openMessage('error', '连接超时，请稍后重试');
            }, 10000); // 10秒超时

            conn.on('data', (data: any) => {
                if (data.type === 'image' || data.type === 'video') {
                    setMessages(prev => [...prev, {
                        sender: conn.peer,
                        content: data.data,
                        timestamp: new Date().toLocaleTimeString(),
                        type: data.type as 'image' | 'video'
                    }]);
                } else {
                    setMessages(prev => [...prev, {
                        sender: conn.peer,
                        content: data.text,
                        timestamp: new Date().toLocaleTimeString(),
                        type: 'text' as const
                    }]);
                }
            });

            conn.on('open', () => {
                clearTimeout(connectionTimeout); // 清除超时定时器
                setConnectedUsers(prev => [...prev, { id: conn.peer, conn }]);
                setMessages(prev => [...prev, {
                    sender: 'System',
                    content: `已连接到 ${conn.peer}`,
                    timestamp: new Date().toLocaleTimeString(),
                    type: 'text' as const
                }]);
                openMessage('success', `已连接到用户 ${conn.peer}`);
            });

            conn.on('error', (err) => {
                clearTimeout(connectionTimeout); // 清除超时定时器
                openMessage('error', `连接失败: ${err.message}`);
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
            timestamp,
            type: 'text' as const
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

    // 处理文件选择
    const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        const reader = new FileReader();
        reader.onload = () => {
            const base64 = reader.result as string;
            let type: 'image' | 'video' = 'image';
            if (file.type.startsWith('video')) type = 'video';
            const timestamp = new Date().toLocaleTimeString();
            const newMessage = {
                sender: activeUser,
                content: base64,
                timestamp,
                type
            };
            setMessages(prev => [...prev, newMessage]);
            // 广播给所有连接用户
            connectedUsers.forEach(({ conn }) => {
                conn.send({ type, data: base64 });
            });
        };
        reader.readAsDataURL(file);
        // 清空 input 以便连续上传同一文件
        e.target.value = '';
    };

    // 关闭图片预览
    const handleClosePreview = () => setPreviewImage(null);

    // 下载图片
    const handleDownloadImage = (imgSrc: string) => {
        const link = document.createElement('a');
        link.href = imgSrc;
        link.download = 'image';
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
    };

    // 发起群聊通话
    const handleStartCall = async () => {
        if (isCalling) return;
        try {
            const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
            setLocalStream(stream);
            setIsCalling(true);
            // 向所有已连接用户发起 call
            connectedUsers.forEach(({ id, conn }) => {
                if (!mediaConnections.current[id]) {
                    const call = peerInstance?.call(id, stream);
                    if (call) {
                        mediaConnections.current[id] = call;
                        call.on('stream', (remoteStream: MediaStream) => {
                            setRemoteStreams(prev => {
                                if (prev.find(s => s.id === id)) return prev;
                                return [...prev, { id, stream: remoteStream }];
                            });
                        });
                        call.on('close', () => {
                            setRemoteStreams(prev => prev.filter(s => s.id !== id));
                            delete mediaConnections.current[id];
                        });
                    }
                }
            });
        } catch (err) {
            openMessage('error', '无法获取摄像头/麦克风权限');
        }
    };

    // 监听接收 call
    useEffect(() => {
        if (!peerInstance) return;
        const handleCall = (call: any) => {
            // 弹窗提示是否同意加入通话
            setCallRequest({ call, visible: true });
        };
        peerInstance.on('call', handleCall);
        return () => {
            peerInstance.off('call', handleCall);
        };
    }, [peerInstance]);

    // 同意加入通话
    const handleAcceptCall = () => {
        if (!callRequest) return;
        navigator.mediaDevices.getUserMedia({ video: true, audio: true }).then(stream => {
            setLocalStream(s => s || stream);
            callRequest.call.answer(stream);
            mediaConnections.current[callRequest.call.peer] = callRequest.call;
            callRequest.call.on('stream', (remoteStream: MediaStream) => {
                setRemoteStreams(prev => {
                    if (prev.find(s => s.id === callRequest.call.peer)) return prev;
                    return [...prev, { id: callRequest.call.peer, stream: remoteStream }];
                });
            });
            callRequest.call.on('close', () => {
                setRemoteStreams(prev => prev.filter(s => s.id !== callRequest.call.peer));
                delete mediaConnections.current[callRequest.call.peer];
            });
            setIsCalling(true);
            setCallRequest(null);
        });
    };
    const handleRejectCall = () => {
        if (!callRequest) return;
        callRequest.call.close && callRequest.call.close();
        setCallRequest(null);
    };

    // 关闭摄像头
    const handleToggleCam = () => {
        if (localStream) {
            localStream.getVideoTracks().forEach(track => {
                track.enabled = !isCamOn;
            });
            setIsCamOn(v => !v);
        }
    };
    // 关闭麦克风
    const handleToggleMic = () => {
        if (localStream) {
            localStream.getAudioTracks().forEach(track => {
                track.enabled = !isMicOn;
            });
            setIsMicOn(v => !v);
        }
    };
    // 挂断通话
    const handleHangUp = () => {
        setIsCalling(false);
        setRemoteStreams([]);
        setIsMicOn(true);
        setIsCamOn(true);
        if (localStream) {
            localStream.getTracks().forEach(track => track.stop());
            setLocalStream(null);
        }
        Object.values(mediaConnections.current).forEach(call => call.close && call.close());
        mediaConnections.current = {};
    };

    // 将本地流赋值给本地 video
    useEffect(() => {
        if (localVideoRef.current && localStream) {
            localVideoRef.current.srcObject = localStream;
        }
    }, [localStream]);
    // 将远端流赋值给远端 video
    useEffect(() => {
        remoteStreams.forEach(({ id, stream }) => {
            const video = remoteVideoRefs.current[id];
            if (video && video.srcObject !== stream) {
                video.srcObject = stream;
            }
        });
    }, [remoteStreams]);

    // 全屏切换
    const handleToggleFullscreen = () => {
        if (!isFullscreen) {
            videoBarRef.current?.requestFullscreen && videoBarRef.current.requestFullscreen();
            setIsFullscreen(true);
        } else {
            document.exitFullscreen && document.exitFullscreen();
            setIsFullscreen(false);
        }
    };
    // 监听全屏变化
    useEffect(() => {
        const onFull = () => setIsFullscreen(!!document.fullscreenElement);
        document.addEventListener('fullscreenchange', onFull);
        return () => document.removeEventListener('fullscreenchange', onFull);
    }, []);

    // 处理页面可见性变化，解决切屏后连接问题
    useEffect(() => {
        const handleVisibilityChange = () => {
            if (document.visibilityState === 'visible') {
                // 页面变为可见时检查连接状态
                connectedUsers.forEach(({ id, conn }) => {
                    // 检查连接是否还有效
                    if (conn.peerConnection && 
                        (conn.peerConnection.connectionState === 'closed' || 
                         conn.peerConnection.connectionState === 'failed' ||
                         conn.peerConnection.connectionState === 'disconnected')) {
                        // 如果连接已关闭，尝试重新连接
                        const newConn = peerInstance?.connect(id);
                        if (newConn) {
                            newConn.on('data', (data: any) => {
                                if (data.type === 'image' || data.type === 'video') {
                                    setMessages(prev => [...prev, {
                                        sender: newConn.peer,
                                        content: data.data,
                                        timestamp: new Date().toLocaleTimeString(),
                                        type: data.type as 'image' | 'video'
                                    }]);
                                } else {
                                    setMessages(prev => [...prev, {
                                        sender: newConn.peer,
                                        content: data.text,
                                        timestamp: new Date().toLocaleTimeString(),
                                        type: 'text' as const
                                    }]);
                                }
                            });

                            newConn.on('open', () => {
                                setConnectedUsers(prev => prev.map(user => 
                                    user.id === id ? { ...user, conn: newConn } : user
                                ));
                                setMessages(prev => [...prev, {
                                    sender: 'System',
                                    content: `与用户 ${id} 的连接已恢复`,
                                    timestamp: new Date().toLocaleTimeString(),
                                    type: 'text' as const
                                }]);
                                openMessage('success', `与用户 ${id} 的连接已恢复`);
                            });
                            
                            newConn.on('error', (err) => {
                                openMessage('error', `重新连接用户 ${id} 失败: ${err.message}`);
                            });
                        }
                    }
                });
            }
        };

        document.addEventListener('visibilitychange', handleVisibilityChange);
        return () => {
            document.removeEventListener('visibilitychange', handleVisibilityChange);
        };
    }, [connectedUsers, peerInstance, openMessage]);

    return (
        <div className="relative flex h-screen bg-gray-900 text-white overflow-hidden">
            {/* 毛玻璃背景 */}
            <GlassEffect />
            {/* 用户列表 */}
            <div
                className={`fixed inset-y-0 right-0 z-50 w-72 bg-gradient-to-br from-gray-800/80 to-gray-900/80 shadow-2xl backdrop-blur-xl p-6 transform
                    ${isMenuOpen ? 'translate-x-0' : 'translate-x-full'}
                    transition-transform duration-300 ease-in-out md:relative md:translate-x-0 md:flex md:flex-col md:h-full border-l border-gray-700`}
                ref={menuRef}
            >
                <h2 className="text-xl font-bold mb-4">我的ID</h2>
                <div className="border p-2 rounded-lg mb-4">
                    <div className="flex justify-between items-center">
                        <p className="text-sm text-gray-300">{myUniqueId}</p>
                        <div className="flex space-x-2">
                            <button
                                onClick={handleCopy}
                                className="ml-2 text-gray-400 hover:text-white transition-colors"
                                aria-label="复制ID"
                            >
                                <CopyOutlined />
                            </button>
                            <button
                                onClick={handleRefresh}
                                className="text-gray-400 hover:text-white transition-colors"
                                aria-label="刷新ID"
                            >
                                <SyncOutlined />
                            </button>
                        </div>
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
                    <div className="flex items-center gap-2">
                        <button
                            className="text-blue-400 hover:text-blue-600 border border-blue-400 rounded px-3 py-1 mr-2"
                            onClick={handleStartCall}
                            disabled={isCalling}
                        >
                            {isCalling ? '通话中' : '发起群聊通话'}
                        </button>
                        <button
                            className="md:hidden text-gray-400 hover:text-white"
                            onClick={() => setIsMenuOpen(!isMenuOpen)}
                        >
                            <MenuOutlined className="w-6 h-6" />
                        </button>
                    </div>
                </div>
                {/* 消息区域 */}
                <div className="flex-1 overflow-y-auto p-4 space-y-4">
                    {messages.map((message, index) => (
                        <div
                            key={index}
                            className={`flex ${message.sender === activeUser ? 'justify-end' : 'justify-start'} animate-fadeInUp`}
                        >
                            <div className={`max-w-xs md:max-w-md rounded-2xl shadow-lg p-4 transition-all duration-300
                                ${message.sender === activeUser
                                    ? 'bg-gradient-to-br from-blue-600/80 to-blue-400/80 text-white rounded-br-none'
                                    : 'bg-gradient-to-br from-gray-700/80 to-gray-900/80 text-white rounded-bl-none'
                                }`}>
                                <div className="font-bold text-sm mb-1">{message.sender}</div>
                                <div className="break-words">
                                    {message.type === 'image' && (
                                        <>
                                            <img
                                                src={message.content}
                                                alt="图片"
                                                className="max-w-full max-h-60 rounded-lg cursor-pointer hover:opacity-80"
                                                onClick={() => setPreviewImage(message.content)}
                                            />
                                            <button
                                                className="mt-1 text-xs text-blue-300 underline hover:text-blue-500"
                                                onClick={() => handleDownloadImage(message.content)}
                                            >下载图片</button>
                                        </>
                                    )}
                                    {message.type === 'video' && (
                                        <video src={message.content} controls className="max-w-full max-h-60 rounded-lg" />
                                    )}
                                    {(!message.type || message.type === 'text') && message.content}
                                </div>
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
                            className="flex-1 bg-gray-800 bg-opacity-60 backdrop-blur-lg text-white rounded-full px-4 py-2 focus:outline-none focus:ring-2 focus:ring-blue-400 transition-all duration-200"
                        />
                        <Button
                            onClick={() => fileInputRef.current?.click()}
                            shape="circle"
                            icon={<UploadOutlined />}
                            className="bg-gray-700 hover:bg-blue-600 text-white"
                            title="发送图片/视频"
                        />
                        <input
                            ref={fileInputRef}
                            type="file"
                            accept="image/*,video/*"
                            style={{ display: 'none' }}
                            onChange={handleFileChange}
                        />
                        <button
                            onClick={handleSendMessage}
                            disabled={!inputValue.trim()}
                            className={`p-2 rounded-full transition-all duration-200 button-active
                                ${inputValue.trim()
                                    ? 'bg-blue-600 hover:bg-blue-700 shadow-md'
                                    : 'bg-gray-700 cursor-not-allowed opacity-60'
                                }`}
                        >
                            <svg className="w-6 h-6 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
                            </svg>
                        </button>
                    </div>
                </div>
            </div>
            {/* 图片预览模态框 */}
            {previewImage && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-black bg-opacity-80" onClick={handleClosePreview}>
                    <div className="relative" onClick={e => e.stopPropagation()}>
                        <img src={previewImage} alt="预览" className="max-w-[90vw] max-h-[80vh] rounded-lg shadow-2xl" />
                        <button
                            className="absolute top-2 right-2 bg-white bg-opacity-80 rounded-full p-2 hover:bg-opacity-100"
                            onClick={() => handleClosePreview()}
                        >
                            <svg className="w-5 h-5 text-gray-800" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                        </button>
                        <button
                            className="absolute bottom-2 right-2 bg-blue-600 text-white rounded px-3 py-1 shadow hover:bg-blue-700"
                            onClick={() => handleDownloadImage(previewImage)}
                        >下载图片</button>
                    </div>
                </div>
            )}
            {/* 视频通话区 */}
            {isCalling && (
                <div
                    ref={videoBarRef}
                    className={`fixed top-0 left-0 w-full flex flex-row gap-4 items-center justify-center bg-black bg-opacity-70 z-40 py-2 ${isFullscreen ? 'h-screen w-screen left-0 top-0 !py-0' : ''}`}
                    style={isFullscreen ? { height: '100vh', width: '100vw', left: 0, top: 0, padding: 0, justifyContent: 'center', alignItems: 'center' } : {}}
                >
                    {/* 本地视频 */}
                    {localStream && (
                        <div className="flex flex-col items-center">
                            <video
                                ref={localVideoRef}
                                autoPlay
                                muted
                                playsInline
                                className={isFullscreen ? 'w-auto h-[80vh] max-w-full max-h-full bg-black rounded-lg border-2 border-blue-400' : 'w-40 h-32 bg-black rounded-lg border-2 border-blue-400'}
                            />
                            <span className="text-xs text-blue-200 mt-1">我</span>
                        </div>
                    )}
                    {/* 远端视频 */}
                    {remoteStreams.map(({ id, stream }) => (
                        <div key={id} className="flex flex-col items-center">
                            <video
                                ref={el => { remoteVideoRefs.current[id] = el; }}
                                autoPlay
                                playsInline
                                className={isFullscreen ? 'w-auto h-[80vh] max-w-full max-h-full bg-black rounded-lg border-2 border-green-400' : 'w-40 h-32 bg-black rounded-lg border-2 border-green-400'}
                            />
                            <span className="text-xs text-green-200 mt-1">{id}</span>
                        </div>
                    ))}
                    {/* 控制按钮 */}
                    <div className="flex flex-col justify-center ml-4 gap-2 absolute right-4 top-4 z-50">
                        <Button onClick={handleToggleMic} shape="circle" icon={isMicOn ? <AudioOutlined /> : <AudioMutedOutlined />} className={isMicOn ? 'bg-blue-500 text-white' : 'bg-gray-500 text-white'} title="麦克风开关" />
                        <Button onClick={handleToggleCam} shape="circle" icon={isCamOn ? <VideoCameraOutlined /> : <VideoCameraAddOutlined />} className={isCamOn ? 'bg-blue-500 text-white' : 'bg-gray-500 text-white'} title="摄像头开关" />
                        <Button onClick={handleHangUp} shape="circle" icon={<CloseOutlined />} className="bg-red-600 text-white" title="挂断" />
                        <Button onClick={handleToggleFullscreen} shape="circle" icon={isFullscreen ? <FullscreenExitOutlined /> : <FullscreenOutlined />} className="bg-gray-700 text-white" title={isFullscreen ? '退出全屏' : '全屏'} />
                    </div>
                </div>
            )}
            {/* 通话请求 Modal */}
            <Modal
                open={!!callRequest?.visible}
                title="群聊通话邀请"
                onOk={handleAcceptCall}
                onCancel={handleRejectCall}
                okText="同意"
                cancelText="拒绝"
                maskClosable={false}
                closable={false}
                centered
            >
                <p>有用户邀请你加入群聊视频通话，是否同意？</p>
            </Modal>
        </div>
    );
}