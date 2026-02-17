/**
 * 在线用户列表组件 - 简约手绘风格
 */

'use client';

import { PeerInfo } from '@/types/room';
import { Crown, User, Sparkles } from 'lucide-react';

interface UserListProps {
  users: PeerInfo[];
  currentUserId: string;
}

export function UserList({ users, currentUserId }: UserListProps) {
  const onlineUsers = users.filter(u => u.isOnline);

  return (
    <div className="h-full">
      <div className="p-4 border-b-2 border-sketch-black">
        <h3 className="font-semibold text-sketch-black font-cave text-lg flex items-center gap-2">
          <Sparkles size={18} />
          在线用户
        </h3>
        <p className="text-sm text-sketch-gray mt-1 font-cave">{onlineUsers.length} 人在线</p>
      </div>

      <div className="p-3 space-y-2 overflow-y-auto h-[calc(100%-73px)]">
        {onlineUsers.map((user) => {
          const isCurrentUser = user.id === currentUserId;

          return (
            <div
              key={user.id}
              className={`flex items-center gap-3 p-3 rounded-sketch transition-all mystic-card border-2 ${
                isCurrentUser
                  ? 'bg-sketch-light border-sketch-black shadow-sketch'
                  : 'hover:bg-sketch-light border-sketch-gray'
              }`}
            >
              {/* 头像 */}
              <div className="relative">
                <div
                  className="w-12 h-12 rounded-full flex items-center justify-center text-white font-semibold text-base shadow-sketch border-2 border-sketch-black"
                  style={{ backgroundColor: user.color }}
                >
                  {user.nickname.charAt(0).toUpperCase()}
                </div>
                <span
                  className={`absolute bottom-0 right-0 w-3.5 h-3.5 rounded-full border-2 border-sketch-background ${
                    user.isOnline ? 'bg-sketch-accent' : 'bg-sketch-gray'
                  }`}
                />
              </div>

              {/* 用户信息 */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <p className="text-base font-medium text-sketch-black truncate font-cave">
                    {user.nickname}
                  </p>
                  {user.isCreator && (
                    <Crown size={16} className="text-sketch-accent flex-shrink-0" />
                  )}
                </div>
                <div className="flex items-center gap-1.5 mt-0.5">
                  <span
                    className={`w-2 h-2 rounded-full ${
                      user.isOnline ? 'bg-sketch-accent' : 'bg-sketch-gray'
                    }`}
                  />
                  <span className="text-xs text-sketch-gray font-cave">
                    {user.isOnline ? '在线' : '离线'}
                  </span>
                </div>
              </div>
            </div>
          );
        })}

        {onlineUsers.length === 0 && (
          <div className="text-center py-12">
            <div className="w-20 h-20 rounded-sketch bg-sketch-light flex items-center justify-center mx-auto mb-4 border-2 border-sketch-black">
              <User size={40} className="text-sketch-gray" />
            </div>
            <p className="text-sketch-gray text-base font-cave">暂无在线用户</p>
            <p className="text-sketch-gray text-sm mt-2 font-cave">分享房间链接邀请他人加入</p>
          </div>
        )}
      </div>
    </div>
  );
}
