/**
 * 聊天组件 - 商业风格
 */

'use client';

import { useState, useRef, useEffect } from 'react';
import { Send, Smile, Image as ImageIcon } from 'lucide-react';
import { Button, Input, Avatar } from 'antd';
import { RoomMessage } from '@/types/room';

interface ChatBoxProps {
  messages: RoomMessage[];
  currentUserId: string;
  onSendMessage: (content: string, type: 'text' | 'image') => void;
  onlineUsers?: any[]; // Yjs awareness users
}

export function ChatBox({ messages, currentUserId, onSendMessage, onlineUsers = [] }: ChatBoxProps) {
  const [input, setInput] = useState('');
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
  const handleSend = () => {
    const content = input.trim();
    if (!content) return;

    onSendMessage(content, 'text');
    setInput('');
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

  return (
    <div className="h-full flex flex-col">
      {/* 消息列表 */}
      <div className="flex-1 overflow-y-auto p-6 space-y-4">
        {messages.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <div className="text-center">
              <div className="w-16 h-16 rounded-full bg-blue-50 flex items-center justify-center mx-auto mb-4">
                <Smile size={32} className="text-blue-400" />
              </div>
              <p className="text-gray-500 font-medium">开始聊天吧...</p>
              <p className="text-gray-400 text-sm mt-1">发送第一条消息来开始对话</p>
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
                  <span className="text-xs text-gray-500 bg-gray-100 px-3 py-1.5 rounded-full">
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
                      className="w-8 h-8 rounded-full flex items-center justify-center text-white text-xs font-medium flex-shrink-0 mt-1"
                      style={{ backgroundColor: displayName === '系统' ? '#6b7280' : '#3b82f6' }}
                    >
                      {displayName.charAt(0).toUpperCase()}
                    </div>
                  )}

                  {/* 消息内容 */}
                  <div className={isCurrentUser ? 'flex flex-col items-end' : 'flex flex-col items-start'}>
                    {!isCurrentUser && (
                      <span className="text-xs text-gray-500 mb-1 ml-1">{displayName}</span>
                    )}
                    <div
                      className={`px-4 py-2.5 rounded-2xl ${
                        isCurrentUser
                          ? 'bg-blue-600 text-white rounded-br-md'
                          : 'bg-gray-100 text-gray-900 rounded-bl-md'
                      }`}
                    >
                      <p className="text-sm whitespace-pre-wrap break-words leading-relaxed">
                        {msg.content}
                      </p>
                    </div>
                    <span className="text-xs text-gray-400 mt-1 px-1">{formatTime(msg.timestamp)}</span>
                  </div>
                </div>
              </div>
            );
          })
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* 输入框 */}
      <div className="p-4 border-t border-gray-200 bg-white">
        <div className="flex items-end gap-3">
          <button className="p-2.5 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors">
            <Smile size={20} />
          </button>
          <button className="p-2.5 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors">
            <ImageIcon size={20} />
          </button>
          <div className="flex-1">
            <Input.TextArea
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入消息... (Enter 发送)"
              autoSize={{ minRows: 1, maxRows: 4 }}
              className="rounded-xl border-gray-300 focus:border-blue-500"
              variant="filled"
            />
          </div>
          <Button
            type="primary"
            icon={<Send size={16} />}
            onClick={handleSend}
            disabled={!input.trim()}
            className="h-11 w-11 rounded-xl bg-blue-600 hover:bg-blue-700 border-none flex items-center justify-center p-0"
          />
        </div>
      </div>
    </div>
  );
}
