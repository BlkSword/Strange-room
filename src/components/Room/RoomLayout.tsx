/**
 * 房间布局组件 - 商业风格
 */

'use client';

import { ReactNode } from 'react';
import { MessageSquare, PenSquare, Code } from 'lucide-react';
import { useState, use } from 'react';

interface RoomLayoutProps {
  children?: ReactNode;
  defaultTab?: 'chat' | 'whiteboard' | 'editor';
  leftPanel?: ReactNode;
  centerPanel?: ReactNode;
  rightPanel?: ReactNode;
  chatPanel?: ReactNode;
  whiteboardPanel?: ReactNode;
  editorPanel?: ReactNode;
}

export function RoomLayout({
  leftPanel,
  chatPanel,
  whiteboardPanel,
  editorPanel,
}: RoomLayoutProps) {
  const [activeTab, setActiveTab] = useState<'chat' | 'whiteboard' | 'editor'>('chat');

  const tabs = [
    { id: 'chat' as const, label: '聊天', icon: MessageSquare, panel: chatPanel },
    { id: 'whiteboard' as const, label: '白板', icon: PenSquare, panel: whiteboardPanel },
    { id: 'editor' as const, label: '代码', icon: Code, panel: editorPanel },
  ];

  return (
    <div className="flex h-[calc(100vh-73px)] bg-gray-50">
      {/* 左侧面板 - 用户列表 */}
      <div className="w-64 border-r border-gray-200 bg-white">
        {leftPanel}
      </div>

      {/* 中间 - 主内容区 */}
      <div className="flex-1 flex flex-col bg-white">
        {/* 标签切换 */}
        <div className="flex items-center gap-1 px-4 py-3 border-b border-gray-200 bg-gray-50">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;

            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-2 px-4 py-2 rounded-lg font-medium transition-all ${
                  isActive
                    ? 'bg-white text-blue-600 shadow-sm'
                    : 'text-gray-600 hover:text-gray-900 hover:bg-white/50'
                }`}
              >
                <Icon size={18} />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* 内容区 */}
        <div className="flex-1 overflow-hidden">
          {activeTab === 'chat' && chatPanel}
          {activeTab === 'whiteboard' && whiteboardPanel}
          {activeTab === 'editor' && editorPanel}
        </div>
      </div>

      {/* 右侧面板 - 文件暂存区 */}
      <div className="w-72 border-l border-gray-200 bg-white">
        <div className="p-4 border-b border-gray-200">
          <h3 className="font-semibold text-gray-900">文件暂存区</h3>
          <p className="text-xs text-gray-500 mt-1">拖拽文件到此处共享</p>
        </div>
        <div className="p-4">
          <div className="border-2 border-dashed border-gray-300 rounded-lg p-8 text-center hover:border-blue-400 hover:bg-blue-50/50 transition-colors cursor-pointer">
            <div className="w-12 h-12 rounded-full bg-gray-100 flex items-center justify-center mx-auto mb-3">
              <PenSquare size={24} className="text-gray-400" />
            </div>
            <p className="text-gray-600 text-sm font-medium">拖拽文件到此处</p>
            <p className="text-gray-400 text-xs mt-1">支持图片、文档等</p>
          </div>
        </div>
      </div>
    </div>
  );
}
