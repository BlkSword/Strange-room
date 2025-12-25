import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  eslint: {
    ignoreDuringBuilds: true,
  },
  // 移除 output: "export" 以支持动态路由
  images: {
    unoptimized: true,
  },
  // 处理hydration错误（浏览器扩展导致）
  experimental: {
    optimizePackageImports: ['lucide-react', 'antd'],
  },
  // 生产环境优化
  productionBrowserSourceMaps: false,
  // 压缩优化
  compress: true,
  // 禁用某些严格检查以支持React 19
  reactStrictMode: false,
};

export default nextConfig;