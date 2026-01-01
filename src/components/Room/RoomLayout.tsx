/**
 * 房间布局组件 - 简约手绘风格
 */

'use client';

import { ReactNode } from 'react';
import { MessageSquare, PenSquare, Code } from 'lucide-react';
import { useState } from 'react';

interface RoomLayoutProps {
  children?: ReactNode;
  defaultTab?: 'chat' | 'whiteboard' | 'editor';
  chatPanel?: ReactNode;
  whiteboardPanel?: ReactNode;
  editorPanel?: ReactNode;
}

export function RoomLayout({
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
    <div className="flex h-[calc(100vh-73px)] bg-sketch-background">
      {/* 左侧标签栏 */}
      <div className="w-20 border-r-2 border-sketch-black bg-sketch-card flex flex-col items-center py-4 gap-3">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          const isActive = activeTab === tab.id;

          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`relative flex flex-col items-center gap-1 p-3 rounded-sketch font-cave transition-all border-2 group ${
                isActive
                  ? 'bg-sketch-background text-sketch-black border-sketch-black shadow-sketch'
                  : 'text-sketch-gray hover:text-sketch-black hover:bg-sketch-light border-transparent hover:border-sketch-black'
              }`}
              title={tab.label}
            >
              <Icon size={24} />
              <span className="text-xs">{tab.label}</span>
            </button>
          );
        })}
      </div>

      {/* 右侧内容区 */}
      <div className="flex-1 overflow-hidden">
        {activeTab === 'chat' && chatPanel}
        {activeTab === 'whiteboard' && whiteboardPanel}
        {activeTab === 'editor' && editorPanel}
      </div>
    </div>
  );
}
