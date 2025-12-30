/**
 * 聊天组件 - 简约手绘风格
 */

'use client';

import { useState, useRef, useEffect } from 'react';
import { Send, Smile, Image as ImageIcon, Lock, Sparkles } from 'lucide-react';
import { Button, Input, Avatar, message } from 'antd';
import { RoomMessage } from '@/types/room';

interface ChatBoxProps {
  messages: RoomMessage[];
  currentUserId: string;
  onSendMessage: (content: string, type: 'text' | 'image') => void;
  onlineUsers?: any[]; // Yjs awareness users
  encryptionEnabled?: boolean;
}

export function ChatBox({ messages, currentUserId, onSendMessage, onlineUsers = [], encryptionEnabled = false }: ChatBoxProps) {
  const [input, setInput] = useState('');
  const [isSending, setIsSending] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // 创建用户ID到用户名的映射
  const userNameMap = new Map(
    onlineUsers.map((u) => [u.user?.id, u.user?.name])
  );

  // 自动滚动到底部
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // 发送消息
  const handleSend = async () => {
    const content = input.trim();
    if (!content) {
      message.warning('请输入消息内容');
      return;
    }

    if (isSending) {
      return;
    }

    setIsSending(true);

    try {
      onSendMessage(content, 'text');
      setInput('');
      // 清除输入框后不显示成功消息，避免打扰
    } catch (error) {
      console.error('[ChatBox] 发送消息失败:', error);
      message.error('发送失败，请重试');
    } finally {
      setIsSending(false);
    }
  };

  // 回车发送
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // 格式化时间
  const formatTime = (timestamp: number): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const diff = now.getTime() - date.getTime();

    if (diff < 60 * 1000) {
      return '刚刚';
    } else if (diff < 60 * 60 * 1000) {
      return `${Math.floor(diff / 60000)} 分钟前`;
    } else if (date.toDateString() === now.toDateString()) {
      return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
    } else {
      return date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
    }
  };

  // 生成用户头像颜色
  const getUserColor = (name: string) => {
    const colors = [
      'bg-sketch-black',
      'bg-sketch-gray',
      'bg-sketch-accent',
      'bg-amber-600',
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
      hash = name.charCodeAt(i) + ((hash << 5) - hash);
    }
    return colors[Math.abs(hash) % colors.length];
  };

  return (
    <div className="h-full flex flex-col">
      {/* 消息列表 */}
      <div className="flex-1 overflow-y-auto p-6 space-y-4">
        {messages.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <div className="text-center">
              <div className="w-20 h-20 rounded-sketch bg-sketch-light flex items-center justify-center mx-auto mb-4 border-2 border-sketch-black">
                <Sparkles size={40} className="text-sketch-gray" />
              </div>
              <p className="text-sketch-gray font-medium font-cave text-xl">开始聊天吧...</p>
              <p className="text-sketch-gray text-base mt-2 font-cave">发送第一条消息来开始对话</p>
            </div>
          </div>
        ) : (
          messages.map((msg) => {
            const isCurrentUser = msg.senderId === currentUserId;
            const isSystem = msg.type === 'system';

            // 从 awareness 或消息本身获取发送者名称
            const displayName = userNameMap.get(msg.senderId) || msg.senderName || '匿名用户';

            if (isSystem) {
              return (
                <div key={msg.id} className="flex justify-center">
                  <span className="text-sm text-sketch-gray bg-sketch-light px-4 py-2 rounded-sketch border-2 border-sketch-black font-cave">
                    {msg.content}
                  </span>
                </div>
              );
            }

            return (
              <div key={msg.id} className={`flex ${isCurrentUser ? 'justify-end' : 'justify-start'}`}>
                <div className={`flex gap-3 max-w-[75%] ${isCurrentUser ? 'flex-row-reverse' : ''}`}>
                  {/* 头像 */}
                  {!isCurrentUser && (
                    <div
                      className={`w-10 h-10 rounded-full flex items-center justify-center text-white text-base font-medium flex-shrink-0 mt-1 border-2 border-sketch-black ${getUserColor(displayName)}`}
                    >
                      {displayName.charAt(0).toUpperCase()}
                    </div>
                  )}

                  {/* 消息内容 */}
                  <div className={isCurrentUser ? 'flex flex-col items-end' : 'flex flex-col items-start'}>
                    {!isCurrentUser && (
                      <span className="text-sm text-sketch-gray mb-1 ml-1 font-cave">{displayName}</span>
                    )}
                    <div
                      className={`px-5 py-3 rounded-sketch border-2 ${
                        isCurrentUser
                          ? 'bg-sketch-black text-sketch-background border-sketch-black shadow-sketch'
                          : 'bg-sketch-light text-sketch-black border-sketch-black'
                      }`}
                    >
                      <div className="flex items-start gap-2">
                        <p className="text-base whitespace-pre-wrap break-words leading-relaxed flex-1 font-cave">
                          {msg.content}
                        </p>
                        {encryptionEnabled && msg.type !== 'system' && (
                          <Lock size={12} className={isCurrentUser ? 'text-sketch-light' : 'text-sketch-gray'} />
                        )}
                      </div>
                    </div>
                    <span className="text-xs text-sketch-gray mt-1 px-1 font-cave">{formatTime(msg.timestamp)}</span>
                  </div>
                </div>
              </div>
            );
          })
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* 输入框 */}
      <div className="p-4 border-t-2 border-sketch-black bg-sketch-card">
        <div className="flex items-end gap-3">
          <button className="p-3 text-sketch-gray hover:text-sketch-black hover:bg-sketch-light rounded-sketch transition-colors border-2 border-transparent hover:border-sketch-black">
            <Smile size={22} />
          </button>
          <button className="p-3 text-sketch-gray hover:text-sketch-black hover:bg-sketch-light rounded-sketch transition-colors border-2 border-transparent hover:border-sketch-black">
            <ImageIcon size={22} />
          </button>
          <div className="flex-1">
            <Input.TextArea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入消息... (Enter 发送)"
              autoSize={{ minRows: 1, maxRows: 4 }}
              className="rounded-sketch border-2 border-sketch-gray focus:border-sketch-black"
            />
          </div>
          <Button
            type="primary"
            icon={<Send size={18} />}
            onClick={handleSend}
            disabled={!input.trim() || isSending}
            loading={isSending}
            className="hand-drawn-btn h-12 w-12 flex items-center justify-center p-0"
          />
        </div>
      </div>
    </div>
  );
}
