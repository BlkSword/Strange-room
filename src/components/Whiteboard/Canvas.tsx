/**
 * 协作白板组件
 */

'use client';

import { useRef, useEffect, useState, useCallback } from 'react';
import { Pen, Eraser, Square, Circle, Trash2, Download } from 'lucide-react';
import type { YjsManager } from '@/lib/yjs/y-doc';

type Tool = 'pen' | 'eraser' | 'rect' | 'circle';

interface WhiteboardProps {
  roomId: string;
  userId: string;
  userName: string;
  yjs: YjsManager | null;
}

interface DrawOperation {
  type: 'pen' | 'rect' | 'circle' | 'eraser';
  points: [number, number][];
  color: string;
  width: number;
}

export function Canvas({ roomId, userId, userName, yjs }: WhiteboardProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [isDrawing, setIsDrawing] = useState(false);
  const [currentTool, setCurrentTool] = useState<Tool>('pen');
  const [color, setColor] = useState('#ffffff');
  const [lineWidth, setLineWidth] = useState(2);
  const [operations, setOperations] = useState<DrawOperation[]>([]);

  // 使用 ref 存储最新的 operations，避免闭包问题
  const operationsRef = useRef<DrawOperation[]>([]);
  operationsRef.current = operations;

  // 当前绘制的路径
  const currentPathRef = useRef<[number, number][]>([]);
  const startPosRef = useRef<{ x: number; y: number } | null>(null);
  const observerRef = useRef<any>(null);

  // 从 Yjs 加载白板数据
  useEffect(() => {
    if (!yjs) return;

    console.log('[Canvas] Yjs 实例已加载');

    // 初始加载
    const loadCanvasData = () => {
      const canvasData = yjs.getRoomInfo('canvasOperations');
      console.log('[Canvas] 加载白板数据:', canvasData);
      if (canvasData) {
        try {
          const ops = typeof canvasData === 'string' ? JSON.parse(canvasData) : canvasData;
          console.log('[Canvas] 解析后的操作:', ops);
          setOperations(ops || []);
        } catch (e) {
          console.error('Failed to parse canvas data:', e);
        }
      } else {
        console.log('[Canvas] 没有现有白板数据');
      }
    };

    loadCanvasData();

    // 直接监听 roomInfoMap 的变化
    if (yjs.roomInfoMap) {
      console.log('[Canvas] 设置 roomInfoMap 监听器');
      observerRef.current = yjs.roomInfoMap.observe((event: any) => {
        console.log('[Canvas] roomInfoMap 变化:', event);
        // 检查是否是 canvasOperations key 变化了
        event.keysChanged.forEach((key: string) => {
          if (key === 'canvasOperations') {
            console.log('[Canvas] canvasOperations 变化，重新加载');
            loadCanvasData();
          }
        });
      });
    }

    return () => {
      if (observerRef.current) {
        console.log('[Canvas] 清理 observer');
        observerRef.current.destroy();
        observerRef.current = null;
      }
    };
  }, [yjs]);

  // 同步操作到 Yjs
  const syncToYjs = useCallback((newOperations: DrawOperation[]) => {
    if (!yjs) return;
    console.log('[Canvas] 同步到 Yjs:', newOperations.length, '个操作');
    yjs.setRoomInfo('canvasOperations', JSON.stringify(newOperations));
  }, [yjs]);

  // 初始化画布
  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const resizeCanvas = () => {
      canvas.width = container.clientWidth;
      canvas.height = container.clientHeight;
      redrawCanvas();
    };

    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    return () => window.removeEventListener('resize', resizeCanvas);
  }, []);

  // 重绘画布 - 使用 ref 获取最新的 operations
  const redrawCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // 清空画布
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // 使用 ref 获取最新的 operations
    const ops = operationsRef.current;

    // 绘制所有操作
    ops.forEach((op) => {
      ctx.strokeStyle = op.color;
      ctx.lineWidth = op.width;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';

      if (op.type === 'pen' || op.type === 'eraser') {
        ctx.beginPath();
        op.points.forEach((point, index) => {
          if (index === 0) {
            ctx.moveTo(point[0], point[1]);
          } else {
            ctx.lineTo(point[0], point[1]);
          }
        });
        ctx.stroke();
      } else if (op.type === 'rect') {
        const start = op.points[0];
        const end = op.points[op.points.length - 1];
        ctx.strokeRect(start[0], start[1], end[0] - start[0], end[1] - start[1]);
      } else if (op.type === 'circle') {
        const start = op.points[0];
        const end = op.points[op.points.length - 1];
        const radius = Math.sqrt(Math.pow(end[0] - start[0], 2) + Math.pow(end[1] - start[1], 2));
        ctx.beginPath();
        ctx.arc(start[0], start[1], radius, 0, Math.PI * 2);
        ctx.stroke();
      }
    });
  }, []); // 不依赖 operations，使用 ref 获取最新值

  // 当 operations 变化时重绘画布
  useEffect(() => {
    console.log('[Canvas] operations 变化，重绘画布，操作数:', operations.length);
    redrawCanvas();
  }, [operations]);

  // 获取鼠标/触摸位置
  const getPosition = useCallback((e: React.MouseEvent | React.TouchEvent): { x: number; y: number } => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };

    const rect = canvas.getBoundingClientRect();
    const clientX = 'touches' in e ? e.touches[0].clientX : e.clientX;
    const clientY = 'touches' in e ? e.touches[0].clientY : e.clientY;

    return {
      x: clientX - rect.left,
      y: clientY - rect.top,
    };
  }, []);

  // 开始绘制
  const handleStart = useCallback((e: React.MouseEvent | React.TouchEvent) => {
    e.preventDefault();
    setIsDrawing(true);

    const pos = getPosition(e);
    currentPathRef.current = [[pos.x, pos.y]];
    startPosRef.current = pos;

    if (currentTool === 'pen' || currentTool === 'eraser') {
      const canvas = canvasRef.current;
      if (!canvas) return;

      const ctx = canvas.getContext('2d');
      if (!ctx) return;

      ctx.strokeStyle = currentTool === 'eraser' ? '#1a1a2e' : color;
      ctx.lineWidth = currentTool === 'eraser' ? 20 : lineWidth;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';

      ctx.beginPath();
      ctx.moveTo(pos.x, pos.y);
    }
  }, [currentTool, color, lineWidth, getPosition]);

  // 绘制中
  const handleMove = useCallback((e: React.MouseEvent | React.TouchEvent) => {
    e.preventDefault();
    if (!isDrawing) return;

    const pos = getPosition(e);
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    if (currentTool === 'pen' || currentTool === 'eraser') {
      ctx.strokeStyle = currentTool === 'eraser' ? '#1a1a2e' : color;
      ctx.lineWidth = currentTool === 'eraser' ? 20 : lineWidth;

      ctx.lineTo(pos.x, pos.y);
      ctx.stroke();

      currentPathRef.current.push([pos.x, pos.y]);
    } else if (currentTool === 'rect' || currentTool === 'circle') {
      // 实时预览
      redrawCanvas();

      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;

      if (currentTool === 'rect' && startPosRef.current) {
        ctx.strokeRect(
          startPosRef.current.x,
          startPosRef.current.y,
          pos.x - startPosRef.current.x,
          pos.y - startPosRef.current.y
        );
      } else if (currentTool === 'circle' && startPosRef.current) {
        const radius = Math.sqrt(
          Math.pow(pos.x - startPosRef.current.x, 2) +
          Math.pow(pos.y - startPosRef.current.y, 2)
        );
        ctx.beginPath();
        ctx.arc(startPosRef.current.x, startPosRef.current.y, radius, 0, Math.PI * 2);
        ctx.stroke();
      }

      currentPathRef.current.push([pos.x, pos.y]);
    }
  }, [isDrawing, currentTool, color, lineWidth, getPosition, redrawCanvas]);

  // 结束绘制
  const handleEnd = useCallback(() => {
    if (!isDrawing) return;
    setIsDrawing(false);

    if (currentPathRef.current.length > 0) {
      const newOp: DrawOperation = {
        type: currentTool,
        points: [...currentPathRef.current],
        color: currentTool === 'eraser' ? '#1a1a2e' : color,
        width: currentTool === 'eraser' ? 20 : lineWidth,
      };

      setOperations((prev) => {
        const newOps = [...prev, newOp];
        syncToYjs(newOps);
        return newOps;
      });
    }

    currentPathRef.current = [];
    startPosRef.current = null;
  }, [isDrawing, currentTool, color, lineWidth, syncToYjs]);

  // 清空画布
  const handleClear = () => {
    setOperations([]);
    syncToYjs([]);
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    ctx?.clearRect(0, 0, canvas.width, canvas.height);
  };

  // 下载画布
  const handleDownload = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const link = document.createElement('a');
    link.download = `whiteboard-${roomId}-${Date.now()}.png`;
    link.href = canvas.toDataURL();
    link.click();
  };

  return (
    <div className="h-full flex flex-col bg-slate-900">
      {/* 工具栏 */}
      <div className="flex items-center gap-2 p-3 border-b border-white/10 bg-black/20">
        <div className="flex items-center gap-1">
          <button
            onClick={() => setCurrentTool('pen')}
            className={`p-2 rounded ${currentTool === 'pen' ? 'bg-blue-500/20 text-blue-400' : 'text-gray-400 hover:text-white'}`}
            title="画笔"
          >
            <Pen size={18} />
          </button>
          <button
            onClick={() => setCurrentTool('eraser')}
            className={`p-2 rounded ${currentTool === 'eraser' ? 'bg-blue-500/20 text-blue-400' : 'text-gray-400 hover:text-white'}`}
            title="橡皮擦"
          >
            <Eraser size={18} />
          </button>
          <button
            onClick={() => setCurrentTool('rect')}
            className={`p-2 rounded ${currentTool === 'rect' ? 'bg-blue-500/20 text-blue-400' : 'text-gray-400 hover:text-white'}`}
            title="矩形"
          >
            <Square size={18} />
          </button>
          <button
            onClick={() => setCurrentTool('circle')}
            className={`p-2 rounded ${currentTool === 'circle' ? 'bg-blue-500/20 text-blue-400' : 'text-gray-400 hover:text-white'}`}
            title="圆形"
          >
            <Circle size={18} />
          </button>
        </div>

        <div className="w-px h-6 bg-white/10 mx-2" />

        {/* 颜色选择 */}
        <div className="flex items-center gap-1">
          {['#ffffff', '#ff6b6b', '#4ecdc4', '#45b7d1', '#f7dc6f', '#bb8fce'].map((c) => (
            <button
              key={c}
              onClick={() => setColor(c)}
              className={`w-6 h-6 rounded ${color === c ? 'ring-2 ring-white' : ''}`}
              style={{ backgroundColor: c }}
              title={c}
            />
          ))}
        </div>

        <div className="w-px h-6 bg-white/10 mx-2" />

        {/* 线宽 */}
        <input
          type="range"
          min={1}
          max={20}
          value={lineWidth}
          onChange={(e) => setLineWidth(Number(e.target.value))}
          className="w-20"
          title="线条粗细"
        />

        <div className="flex-1" />

        <button
          onClick={handleClear}
          className="p-2 text-gray-400 hover:text-red-400 transition-colors"
          title="清空画布"
        >
          <Trash2 size={18} />
        </button>
        <button
          onClick={handleDownload}
          className="p-2 text-gray-400 hover:text-white transition-colors"
          title="下载画布"
        >
          <Download size={18} />
        </button>
      </div>

      {/* 画布区域 */}
      <div ref={containerRef} className="flex-1 relative overflow-hidden">
        <canvas
          ref={canvasRef}
          onMouseDown={handleStart}
          onMouseMove={handleMove}
          onMouseUp={handleEnd}
          onMouseLeave={handleEnd}
          onTouchStart={handleStart}
          onTouchMove={handleMove}
          onTouchEnd={handleEnd}
          className="absolute inset-0 cursor-crosshair touch-none"
        />

        {/* 提示信息 */}
        {operations.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <p className="text-gray-500 text-sm">开始绘制...</p>
          </div>
        )}
      </div>
    </div>
  );
}
