/**
 * 协作白板组件 - 现代化重构版本
 */

'use client';

import { useRef, useEffect, useState, useCallback } from 'react';
import {
  Pen, Eraser, Square, Circle, Minus, Type, Trash2, Download,
  Undo2, Redo2, Palette, MinusCircle
} from 'lucide-react';
import { message, Dropdown, ColorPicker, Slider } from 'antd';
import type { YjsManager } from '@/lib/yjs/y-doc';

type Tool =
  | 'pen'
  | 'eraser'
  | 'rect'
  | 'circle'
  | 'line'
  | 'arrow'
  | 'text'
  | 'fill';

interface WhiteboardProps {
  roomId: string;
  userId: string;
  userName: string;
  yjs: YjsManager | null;
}

interface DrawOperation {
  type: Tool;
  points: [number, number][];
  color: string;
  width: number;
  fill?: boolean;
  text?: string;
}

export function Canvas({ roomId, userId, userName, yjs }: WhiteboardProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [isDrawing, setIsDrawing] = useState(false);
  const [currentTool, setCurrentTool] = useState<Tool>('pen');
  const [color, setColor] = useState('#ffffff');
  const [lineWidth, setLineWidth] = useState(3);
  const [operations, setOperations] = useState<DrawOperation[]>([]);
  const [history, setHistory] = useState<DrawOperation[][]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [isFill, setIsFill] = useState(false);
  const [showColorPicker, setShowColorPicker] = useState(false);
  const [textInput, setTextInput] = useState<{ x: number; y: number } | null>(null);
  const [textValue, setTextValue] = useState('');

  // 使用 ref 存储最新的 operations，避免闭包问题
  const operationsRef = useRef<DrawOperation[]>([]);
  operationsRef.current = operations;

  // 使用 ref 存储 history 和 historyIndex，供快捷键使用
  const historyRef = useRef<DrawOperation[][]>([]);
  const historyIndexRef = useRef(-1);
  historyRef.current = history;
  historyIndexRef.current = historyIndex;

  // 当前绘制的路径
  const currentPathRef = useRef<[number, number][]>([]);
  const startPosRef = useRef<{ x: number; y: number } | null>(null);

  // 预设颜色
  const presetColors = [
    '#ffffff', '#ff6b6b', '#ee5a6f', '#ff8787',
    '#4ecdc4', '#45b7d1', '#26a69a', '#80cbc4',
    '#f7dc6f', '#ffd93d', '#ffbe0b', '#ffb703',
    '#bb8fce', '#9b59b6', '#8e44ad', '#6c5ce7',
    '#95a5a6', '#7f8c8d', '#34495e', '#2c3e50',
  ];

  // 从 Yjs 加载白板数据
  const initialLoadRef = useRef(false);
  useEffect(() => {
    if (!yjs) return;

    const loadCanvasData = () => {
      const canvasData = yjs.getRoomInfo('canvasOperations');
      if (canvasData) {
        try {
          const ops = typeof canvasData === 'string' ? JSON.parse(canvasData) : canvasData;
          setOperations(ops || []);
          // 只在初始加载时设置历史记录
          if (!initialLoadRef.current) {
            setHistory([ops || []]);
            setHistoryIndex(0);
            initialLoadRef.current = true;
          }
        } catch (e) {
          console.error('Failed to parse canvas data:', e);
        }
      }
    };

    loadCanvasData();

    const handleUpdate = () => {
      const canvasData = yjs.getRoomInfo('canvasOperations');
      if (canvasData) {
        try {
          const ops = typeof canvasData === 'string' ? JSON.parse(canvasData) : canvasData;
          setOperations(ops || []);
          // 不重置历史记录，只更新操作
        } catch (e) {
          console.error('[Canvas] 解析 canvasData 失败:', e);
        }
      }
    };

    yjs.doc.on('update', handleUpdate);

    return () => {
      yjs.doc.off('update', handleUpdate);
    };
  }, [yjs]);

  // 同步操作到 Yjs
  const syncToYjs = useCallback((newOperations: DrawOperation[]) => {
    if (!yjs) return;
    yjs.setRoomInfo('canvasOperations', JSON.stringify(newOperations));
  }, [yjs]);

  // 节流同步
  const throttledSyncRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const throttledSyncToYjs = useCallback((newOperations: DrawOperation[]) => {
    if (throttledSyncRef.current) {
      clearTimeout(throttledSyncRef.current);
    }
    throttledSyncRef.current = setTimeout(() => {
      syncToYjs(newOperations);
      throttledSyncRef.current = null;
    }, 100);
  }, [syncToYjs]);

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

    return () => {
      window.removeEventListener('resize', resizeCanvas);
      if (throttledSyncRef.current) {
        clearTimeout(throttledSyncRef.current);
      }
    };
  }, [throttledSyncRef]);

  // 重绘画布
  const redrawCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const ops = operationsRef.current;

    ops.forEach((op) => {
      ctx.strokeStyle = op.color;
      ctx.lineWidth = op.width;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';

      if (op.type === 'pen') {
        ctx.beginPath();
        op.points.forEach((point, index) => {
          if (index === 0) ctx.moveTo(point[0], point[1]);
          else ctx.lineTo(point[0], point[1]);
        });
        ctx.stroke();
      } else if (op.type === 'eraser') {
        ctx.globalCompositeOperation = 'destination-out';
        ctx.beginPath();
        op.points.forEach((point, index) => {
          if (index === 0) ctx.moveTo(point[0], point[1]);
          else ctx.lineTo(point[0], point[1]);
        });
        ctx.stroke();
        ctx.globalCompositeOperation = 'source-over';
      } else if (op.type === 'rect') {
        const start = op.points[0];
        const end = op.points[op.points.length - 1];
        const w = end[0] - start[0];
        const h = end[1] - start[1];

        if (op.fill) {
          ctx.fillStyle = op.color;
          ctx.fillRect(start[0], start[1], w, h);
        }
        ctx.strokeRect(start[0], start[1], w, h);
      } else if (op.type === 'circle') {
        const start = op.points[0];
        const end = op.points[op.points.length - 1];
        const radius = Math.sqrt(Math.pow(end[0] - start[0], 2) + Math.pow(end[1] - start[1], 2));

        ctx.beginPath();
        ctx.arc(start[0], start[1], radius, 0, Math.PI * 2);
        if (op.fill) {
          ctx.fillStyle = op.color;
          ctx.fill();
        }
        ctx.stroke();
      } else if (op.type === 'line') {
        const start = op.points[0];
        const end = op.points[op.points.length - 1];
        ctx.beginPath();
        ctx.moveTo(start[0], start[1]);
        ctx.lineTo(end[0], end[1]);
        ctx.stroke();
      } else if (op.type === 'arrow') {
        const start = op.points[0];
        const end = op.points[op.points.length - 1];
        const angle = Math.atan2(end[1] - start[1], end[0] - start[0]);
        const arrowSize = op.width * 4;

        ctx.beginPath();
        ctx.moveTo(start[0], start[1]);
        ctx.lineTo(end[0], end[1]);
        ctx.stroke();

        // 箭头
        ctx.beginPath();
        ctx.moveTo(end[0], end[1]);
        ctx.lineTo(
          end[0] - arrowSize * Math.cos(angle - Math.PI / 6),
          end[1] - arrowSize * Math.sin(angle - Math.PI / 6)
        );
        ctx.moveTo(end[0], end[1]);
        ctx.lineTo(
          end[0] - arrowSize * Math.cos(angle + Math.PI / 6),
          end[1] - arrowSize * Math.sin(angle + Math.PI / 6)
        );
        ctx.stroke();
      } else if (op.type === 'text' && op.text) {
        ctx.fillStyle = op.color;
        ctx.font = `${op.width * 5}px Arial`;
        ctx.fillText(op.text, op.points[0][0], op.points[0][1]);
      }
    });
  }, []);

  useEffect(() => {
    redrawCanvas();
  }, [operations, redrawCanvas]);

  // 添加到历史记录
  const addToHistory = useCallback((newOps: DrawOperation[]) => {
    const currentIndex = historyIndexRef.current;
    const currentHistory = historyRef.current;

    const newHistory = currentHistory.slice(0, currentIndex + 1);
    newHistory.push(newOps);

    setHistory(newHistory);
    setHistoryIndex(newHistory.length - 1);
  }, []);

  // 撤销
  const undo = useCallback(() => {
    console.log('[Canvas] undo 被调用, historyIndex:', historyIndexRef.current, 'history.length:', historyRef.current.length);
    if (historyIndexRef.current > 0) {
      const newIndex = historyIndexRef.current - 1;
      setHistoryIndex(newIndex);
      const prevOps = historyRef.current[newIndex];
      setOperations(prevOps);
      syncToYjs(prevOps);
      console.log('[Canvas] undo 执行成功, newIndex:', newIndex, 'ops.length:', prevOps.length);
    } else {
      console.log('[Canvas] undo 无法执行: historyIndex <= 0');
    }
  }, [syncToYjs]);

  // 重做
  const redo = useCallback(() => {
    console.log('[Canvas] redo 被调用, historyIndex:', historyIndexRef.current, 'history.length:', historyRef.current.length);
    if (historyIndexRef.current < historyRef.current.length - 1) {
      const newIndex = historyIndexRef.current + 1;
      setHistoryIndex(newIndex);
      const nextOps = historyRef.current[newIndex];
      setOperations(nextOps);
      syncToYjs(nextOps);
      console.log('[Canvas] redo 执行成功, newIndex:', newIndex, 'ops.length:', nextOps.length);
    } else {
      console.log('[Canvas] redo 无法执行: historyIndex >= history.length - 1');
    }
  }, [syncToYjs]);

  // 全局键盘监听器，确保快捷键始终可用
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // 如果正在输入文字，不处理快捷键
      if (textInput) return;

      // 检查是否在输入框中
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
        return;
      }

      if (e.ctrlKey || e.metaKey) {
        if (e.key === 'z') {
          e.preventDefault();
          console.log('[Canvas] Ctrl+Z 触发, historyIndex:', historyIndexRef.current, 'history.length:', historyRef.current.length);
          undo();
        } else if (e.key === 'y') {
          e.preventDefault();
          console.log('[Canvas] Ctrl+Y 触发, historyIndex:', historyIndexRef.current, 'history.length:', historyRef.current.length);
          redo();
        }
      } else {
        const num = parseInt(e.key);
        if (num >= 1 && num <= 7) {
          const toolMap: Record<number, Tool> = {
            1: 'pen', 2: 'eraser', 3: 'line', 4: 'arrow',
            5: 'rect', 6: 'circle', 7: 'text',
          };
          setCurrentTool(toolMap[num]);
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [textInput, undo, redo]);

  // 获取位置
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
    if (currentTool === 'text') {
      const pos = getPosition(e);
      setTextInput(pos);
      setTextValue('');
      return;
    }

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

      ctx.strokeStyle = color;
      ctx.lineWidth = currentTool === 'eraser' ? lineWidth * 3 : lineWidth;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';

      if (currentTool === 'eraser') {
        ctx.globalCompositeOperation = 'destination-out';
      }

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
      ctx.lineWidth = currentTool === 'eraser' ? lineWidth * 3 : lineWidth;
      ctx.lineTo(pos.x, pos.y);
      ctx.stroke();
      currentPathRef.current.push([pos.x, pos.y]);
    } else {
      redrawCanvas();

      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';

      if (currentTool === 'rect' && startPosRef.current) {
        const w = pos.x - startPosRef.current.x;
        const h = pos.y - startPosRef.current.y;
        if (isFill) {
          ctx.fillStyle = color;
          ctx.fillRect(startPosRef.current.x, startPosRef.current.y, w, h);
        }
        ctx.strokeRect(startPosRef.current.x, startPosRef.current.y, w, h);
      } else if (currentTool === 'circle' && startPosRef.current) {
        const radius = Math.sqrt(
          Math.pow(pos.x - startPosRef.current.x, 2) +
          Math.pow(pos.y - startPosRef.current.y, 2)
        );
        ctx.beginPath();
        ctx.arc(startPosRef.current.x, startPosRef.current.y, radius, 0, Math.PI * 2);
        if (isFill) {
          ctx.fillStyle = color;
          ctx.fill();
        }
        ctx.stroke();
      } else if (currentTool === 'line' && startPosRef.current) {
        ctx.beginPath();
        ctx.moveTo(startPosRef.current.x, startPosRef.current.y);
        ctx.lineTo(pos.x, pos.y);
        ctx.stroke();
      } else if (currentTool === 'arrow' && startPosRef.current) {
        const angle = Math.atan2(pos.y - startPosRef.current.y, pos.x - startPosRef.current.x);
        const arrowSize = lineWidth * 4;

        ctx.beginPath();
        ctx.moveTo(startPosRef.current.x, startPosRef.current.y);
        ctx.lineTo(pos.x, pos.y);
        ctx.stroke();

        ctx.beginPath();
        ctx.moveTo(pos.x, pos.y);
        ctx.lineTo(
          pos.x - arrowSize * Math.cos(angle - Math.PI / 6),
          pos.y - arrowSize * Math.sin(angle - Math.PI / 6)
        );
        ctx.moveTo(pos.x, pos.y);
        ctx.lineTo(
          pos.x - arrowSize * Math.cos(angle + Math.PI / 6),
          pos.y - arrowSize * Math.sin(angle + Math.PI / 6)
        );
        ctx.stroke();
      }

      currentPathRef.current.push([pos.x, pos.y]);
    }
  }, [isDrawing, currentTool, color, lineWidth, isFill, getPosition, redrawCanvas]);

  // 结束绘制
  const handleEnd = useCallback(() => {
    if (!isDrawing) return;
    setIsDrawing(false);

    if (currentPathRef.current.length > 0) {
      const newOp: DrawOperation = {
        type: currentTool,
        points: [...currentPathRef.current],
        color: color,
        width: currentTool === 'eraser' ? lineWidth * 3 : lineWidth,
        fill: isFill,
      };

      setOperations((prev) => {
        const newOps = [...prev, newOp];
        addToHistory(newOps);
        throttledSyncToYjs(newOps);
        return newOps;
      });
    }

    currentPathRef.current = [];
    startPosRef.current = null;
  }, [isDrawing, currentTool, color, lineWidth, isFill, addToHistory, throttledSyncToYjs]);

  // 处理文字输入
  const handleTextSubmit = () => {
    if (!textInput || !textValue.trim()) {
      setTextInput(null);
      setTextValue('');
      return;
    }

    const newOp: DrawOperation = {
      type: 'text',
      points: [[textInput.x, textInput.y]],
      color: color,
      width: lineWidth,
      text: textValue,
    };

    setOperations((prev) => {
      const newOps = [...prev, newOp];
      addToHistory(newOps);
      throttledSyncToYjs(newOps);
      return newOps;
    });

    setTextInput(null);
    setTextValue('');
  };

  // 清空画布
  const handleClear = () => {
    const newOps: DrawOperation[] = [];
    setOperations(newOps);
    addToHistory(newOps);
    syncToYjs(newOps);
    message.success('画布已清空');
  };

  // 下载画布
  const handleDownload = () => {
    const canvas = canvasRef.current;
    if (!canvas) {
      message.error('下载失败：画布未就绪');
      return;
    }

    try {
      const link = document.createElement('a');
      link.download = `whiteboard-${roomId}-${Date.now()}.png`;
      link.href = canvas.toDataURL('image/png');
      link.click();
      message.success('画布已下载');
    } catch (error) {
      console.error('[Canvas] 下载画布失败:', error);
      message.error('下载失败，请重试');
    }
  };

  // 工具按钮配置
  const tools = [
    { id: 'pen', icon: Pen, label: '画笔' },
    { id: 'eraser', icon: Eraser, label: '橡皮擦' },
    { id: 'line', icon: Minus, label: '直线' },
    { id: 'arrow', icon: MinusCircle, label: '箭头' },
    { id: 'rect', icon: Square, label: '矩形' },
    { id: 'circle', icon: Circle, label: '圆形' },
    { id: 'text', icon: Type, label: '文字' },
  ] as const;

  const colorPickerContent = (
    <div className="p-3 bg-gray-900 rounded-lg shadow-xl border border-gray-700">
      <div className="grid grid-cols-5 gap-2 mb-3">
        {presetColors.map((c) => (
          <button
            key={c}
            onClick={() => setColor(c)}
            className={`w-8 h-8 rounded-lg transition-transform hover:scale-110 ${
              color === c ? 'ring-2 ring-white ring-offset-2 ring-offset-gray-900' : ''
            }`}
            style={{ backgroundColor: c }}
            title={c}
          />
        ))}
      </div>
      <ColorPicker
        value={color}
        onChange={(color) => setColor(color.toHexString())}
        showText
        className="w-full"
      />
    </div>
  );

  return (
    <div className="h-full flex flex-col bg-slate-900">
      {/* 主工具栏 */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-white/10 bg-black/30 backdrop-blur-sm">
        {/* 左侧工具组 */}
        <div className="flex items-center gap-1">
          {tools.map((tool) => {
            const Icon = tool.icon;
            return (
              <button
                key={tool.id}
                onClick={() => setCurrentTool(tool.id as Tool)}
                className={`p-2.5 rounded-lg transition-all duration-200 ${
                  currentTool === tool.id
                    ? 'bg-blue-500/20 text-blue-400 shadow-lg shadow-blue-500/20'
                    : 'text-gray-400 hover:text-white hover:bg-white/5'
                }`}
                title={tool.label}
              >
                <Icon size={20} />
              </button>
            );
          })}
        </div>

        <div className="w-px h-8 bg-white/10 mx-2" />

        {/* 颜色选择 */}
        <Dropdown
          open={showColorPicker}
          onOpenChange={setShowColorPicker}
          dropdownRender={() => colorPickerContent}
          trigger={['click']}
        >
          <button
            className={`p-2 rounded-lg transition-all duration-200 ${
              showColorPicker
                ? 'bg-purple-500/20 text-purple-400'
                : 'text-gray-400 hover:text-white hover:bg-white/5'
            }`}
            title="颜色"
          >
            <Palette size={20} />
          </button>
        </Dropdown>

        {/* 当前颜色预览 */}
        <button
          onClick={() => setShowColorPicker(!showColorPicker)}
          className="w-8 h-8 rounded-lg ring-2 ring-white/20 hover:ring-white/40 transition-all"
          style={{ backgroundColor: color }}
          title="点击选择颜色"
        />

        <div className="w-px h-8 bg-white/10 mx-2" />

        {/* 线宽滑块 */}
        <div className="flex items-center gap-3 px-2">
          <span className="text-xs text-gray-500">粗细</span>
          <Slider
            min={1}
            max={20}
            value={lineWidth}
            onChange={(v) => setLineWidth(v as number)}
            className="w-24"
            tooltip={{ formatter: (v) => `${v}px` }}
          />
          <span className="text-xs text-gray-400 w-8">{lineWidth}px</span>
        </div>

        {/* 填充选项 */}
        {(currentTool === 'rect' || currentTool === 'circle') && (
          <>
            <div className="w-px h-8 bg-white/10 mx-2" />
            <button
              onClick={() => setIsFill(!isFill)}
              className={`px-3 py-1.5 rounded-lg text-sm font-medium transition-all duration-200 ${
                isFill
                  ? 'bg-green-500/20 text-green-400'
                  : 'text-gray-400 hover:text-white hover:bg-white/5'
              }`}
              title="切换填充"
            >
              填充
            </button>
          </>
        )}

        <div className="flex-1" />

        {/* 撤销/重做 */}
        <div className="flex items-center gap-1">
          <button
            onClick={undo}
            disabled={historyIndex <= 0}
            className={`p-2 rounded-lg transition-all duration-200 ${
              historyIndex > 0
                ? 'text-gray-400 hover:text-white hover:bg-white/5'
                : 'text-gray-600 cursor-not-allowed'
            }`}
            title="撤销 (Ctrl+Z)"
          >
            <Undo2 size={18} />
          </button>
          <button
            onClick={redo}
            disabled={historyIndex >= history.length - 1}
            className={`p-2 rounded-lg transition-all duration-200 ${
              historyIndex < history.length - 1
                ? 'text-gray-400 hover:text-white hover:bg-white/5'
                : 'text-gray-600 cursor-not-allowed'
            }`}
            title="重做 (Ctrl+Y)"
          >
            <Redo2 size={18} />
          </button>
        </div>

        <div className="w-px h-8 bg-white/10 mx-2" />

        {/* 清空和下载 */}
        <button
          onClick={handleClear}
          className="p-2.5 rounded-lg text-gray-400 hover:text-red-400 hover:bg-red-500/10 transition-all duration-200"
          title="清空画布"
        >
          <Trash2 size={20} />
        </button>
        <button
          onClick={handleDownload}
          className="p-2.5 rounded-lg text-gray-400 hover:text-green-400 hover:bg-green-500/10 transition-all duration-200"
          title="下载画布"
        >
          <Download size={20} />
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
          className={`absolute inset-0 touch-none ${
            currentTool === 'eraser' ? 'cursor-cell' :
            currentTool === 'text' ? 'cursor-text' :
            'cursor-crosshair'
          }`}
        />

        {/* 文字输入框 */}
        {textInput && (
          <div
            className="absolute"
            style={{
              left: textInput.x,
              top: textInput.y,
              transform: 'translate(-4px, -50%)',
            }}
          >
            <input
              type="text"
              value={textValue}
              onChange={(e) => setTextValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleTextSubmit();
                if (e.key === 'Escape') {
                  setTextInput(null);
                  setTextValue('');
                }
              }}
              onBlur={handleTextSubmit}
              autoFocus
              className="px-3 py-2 bg-gray-800 text-white border-2 border-blue-500 rounded-lg outline-none text-base min-w-[200px]"
              placeholder="输入文字..."
              style={{ color: color }}
            />
          </div>
        )}

        {/* 空状态提示 */}
        {operations.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <div className="text-center">
              <div className="w-24 h-24 mx-auto mb-4 rounded-full bg-gradient-to-br from-blue-500/20 to-purple-500/20 flex items-center justify-center">
                <Pen size={40} className="text-blue-400" />
              </div>
              <p className="text-gray-400 text-lg mb-2">开始创作</p>
              <p className="text-gray-600 text-sm">选择工具在画布上绘制</p>
            </div>
          </div>
        )}

        {/* 画笔大小预览 */}
        {isDrawing && (currentTool === 'pen' || currentTool === 'eraser') && (
          <div
            className="fixed pointer-events-none rounded-full border border-white/30 z-50"
            style={{
              width: currentTool === 'eraser' ? lineWidth * 3 : lineWidth,
              height: currentTool === 'eraser' ? lineWidth * 3 : lineWidth,
              backgroundColor: currentTool === 'eraser' ? 'rgba(255,255,255,0.3)' : color,
              transform: 'translate(-50%, -50%)',
            }}
          />
        )}

        {/* 快捷键提示 */}
        <div className="absolute bottom-4 left-4 px-3 py-2 bg-black/50 backdrop-blur-sm rounded-lg text-xs text-gray-400">
          <div className="flex items-center gap-4">
            <span>Ctrl+Z 撤销</span>
            <span>Ctrl+Y 重做</span>
            <span className="text-gray-500">|</span>
            <span>工具: 1-7</span>
          </div>
        </div>
      </div>
    </div>
  );
}
