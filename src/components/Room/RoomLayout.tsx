/**
 * 房间布局组件 - 简约手绘风格
 */

'use client';

import { ReactNode } from 'react';
import { MessageSquare, PenSquare, Code, Sparkles } from 'lucide-react';
import { useState } from 'react';

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
    <div className="flex h-[calc(100vh-73px)] bg-sketch-background">
      {/* 左侧面板 - 用户列表 */}
      <div className="w-64 border-r-2 border-sketch-black bg-sketch-card">
        {leftPanel}
      </div>

      {/* 中间 - 主内容区 */}
      <div className="flex-1 flex flex-col bg-sketch-background">
        {/* 标签切换 */}
        <div className="flex items-center gap-2 px-4 py-3 border-b-2 border-sketch-black bg-sketch-card">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;

            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-2 px-5 py-2.5 rounded-sketch font-cave text-base transition-all border-2 ${
                  isActive
                    ? 'bg-sketch-background text-sketch-black border-sketch-black shadow-sketch'
                    : 'text-sketch-gray hover:text-sketch-black hover:bg-sketch-light border-transparent hover:border-sketch-black'
                }`}
              >
                <Icon size={18} />
                <span>{tab.label}</span>
                {isActive && <Sparkles size={14} className="text-sketch-black" />}
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
      <div className="w-72 border-l-2 border-sketch-black bg-sketch-card">
        <div className="p-4 border-b-2 border-sketch-light">
          <h3 className="font-semibold text-sketch-black font-cave text-lg flex items-center gap-2">
            <Sparkles size={18} />
            文件暂存区
          </h3>
          <p className="text-sm text-sketch-gray mt-1 font-cave">拖拽文件到此处共享</p>
        </div>
        <div className="p-4">
          <div className="border-2 border-dashed border-sketch-gray rounded-sketch p-8 text-center hover:border-sketch-black hover:bg-sketch-light transition-all cursor-pointer mystic-card">
            <div className="w-14 h-14 rounded-sketch bg-sketch-light flex items-center justify-center mx-auto mb-3 border-2 border-sketch-black">
              <PenSquare size={28} className="text-sketch-black" />
            </div>
            <p className="text-sketch-gray text-base font-medium font-cave">拖拽文件到此处</p>
            <p className="text-sketch-gray text-sm mt-1 font-cave">支持图片、文档等</p>
          </div>
        </div>
      </div>
    </div>
  );
}
