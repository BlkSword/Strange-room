/**
 * 在线用户列表组件 - 商业风格
 */

'use client';

import { PeerInfo } from '@/types/room';
import { Crown, User } from 'lucide-react';
import { Avatar, Badge } from 'antd';

interface UserListProps {
  users: PeerInfo[];
  currentUserId: string;
}

export function UserList({ users, currentUserId }: UserListProps) {
  const onlineUsers = users.filter(u => u.isOnline);

  return (
    <div className="h-full">
      <div className="p-4 border-b border-gray-200">
        <h3 className="font-semibold text-gray-900">在线用户</h3>
        <p className="text-xs text-gray-500 mt-1">{onlineUsers.length} 人在线</p>
      </div>

      <div className="p-3 space-y-1 overflow-y-auto h-[calc(100%-73px)]">
        {onlineUsers.map((user) => {
          const isCurrentUser = user.id === currentUserId;

          return (
            <div
              key={user.id}
              className={`flex items-center gap-3 p-3 rounded-xl transition-all ${
                isCurrentUser
                  ? 'bg-blue-50 border border-blue-200'
                  : 'hover:bg-gray-50 border border-transparent'
              }`}
            >
              {/* 头像 */}
              <div className="relative">
                <div
                  className="w-10 h-10 rounded-full flex items-center justify-center text-white font-semibold text-sm shadow-sm"
                  style={{ backgroundColor: user.color }}
                >
                  {user.nickname.charAt(0).toUpperCase()}
                </div>
                <span
                  className={`absolute bottom-0 right-0 w-3 h-3 rounded-full border-2 border-white ${
                    user.isOnline ? 'bg-emerald-500' : 'bg-gray-300'
                  }`}
                />
              </div>

              {/* 用户信息 */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <p className="text-sm font-medium text-gray-900 truncate">
                    {user.nickname}
                  </p>
                  {user.isCreator && (
                    <Crown size={14} className="text-amber-500 flex-shrink-0" />
                  )}
                </div>
                <div className="flex items-center gap-1.5 mt-0.5">
                  <span
                    className={`w-1.5 h-1.5 rounded-full ${
                      user.isOnline ? 'bg-emerald-500' : 'bg-gray-300'
                    }`}
                  />
                  <span className="text-xs text-gray-500">
                    {user.isOnline ? '在线' : '离线'}
                  </span>
                </div>
              </div>
            </div>
          );
        })}

        {onlineUsers.length === 0 && (
          <div className="text-center py-12">
            <div className="w-16 h-16 rounded-full bg-gray-100 flex items-center justify-center mx-auto mb-4">
              <User size={32} className="text-gray-400" />
            </div>
            <p className="text-gray-500 text-sm">暂无在线用户</p>
            <p className="text-gray-400 text-xs mt-1">分享房间链接邀请他人加入</p>
          </div>
        )}
      </div>
    </div>
  );
}
