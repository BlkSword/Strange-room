'use client';

import { App } from 'antd';

export function Providers({ children }: { children: React.ReactNode }) {
  return <App>{children}</App>;
}
