'use client';

import { App, message } from 'antd';
import { useEffect } from 'react';

// 配置全局 message
message.config({
  top: 24,
  duration: 3,
  maxCount: 3,
  prefixCls: 'ant-message',
});

export function Providers({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    // 确保 message 容器在客户端渲染
    message.config({
      top: 24,
      duration: 3,
      maxCount: 3,
    });
  }, []);

  return (
    <App
      message={{
        top: 24,
        duration: 3,
        maxCount: 3,
        rtl: false,
      }}
    >
      {children}
    </App>
  );
}
