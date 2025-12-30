"use client";

import Link from "next/link";
import { Home } from "lucide-react";
import { Button } from "antd";

export default function RoomNotFound() {
  return (
    <div className="min-h-screen bg-sketch-background flex items-center justify-center px-6">
      <div className="text-center max-w-lg">
        {/* 标题 */}
        <h1 className="text-5xl md:text-6xl font-marker mb-6 leading-tight hand-drawn-title text-sketch-black">
          页面未找到
        </h1>

        {/* 描述 */}
        <p className="text-xl text-sketch-gray mb-8 font-cave">
          抱歉，您访问的页面不存在或已被移除
        </p>

        {/* 返回首页按钮 */}
        <Link href="/">
          <Button
            type="primary"
            size="large"
            icon={<Home size={20} />}
            className="hand-drawn-btn text-lg px-8"
          >
            返回首页
          </Button>
        </Link>
      </div>
    </div>
  );
}
