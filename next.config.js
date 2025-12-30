/** @type {import('next').NextConfig} */
const nextConfig = {
  eslint: {
    ignoreDuringBuilds: true,
  },
  images: {
    unoptimized: true,
  },
  experimental: {
    optimizePackageImports: ['lucide-react', 'antd'],
  },
  productionBrowserSourceMaps: false,
  compress: true,
  reactStrictMode: false,
};

module.exports = nextConfig;
