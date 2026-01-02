/**
 * 轻量级离线代码编辑器
 * 无需外部资源，完全本地运行
 */

'use client';

import { useState, useRef, useEffect } from 'react';
import { Download, Copy } from 'lucide-react';
import { message } from 'antd';

interface CodeEditorProps {
  roomId: string;
  userId: string;
  userName: string;
  initialCode?: string;
  onCodeChange?: (code: string) => void;
  otherUsers?: Array<{ id: string; name: string; color: string; selection?: { from: number; to: number } }>;
}

// 简单的语法高亮
const highlightSyntax = (code: string, language: string): string => {
  // HTML 转义
  let escaped = code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

  // 基础语法高亮规则 - 使用内联样式
  const patterns = [
    // 字符串
    { regex: /(".*?"|'.*?'|`.*?`)/g, style: 'color: #a5d6ff;' },
    // 注释
    { regex: /(\/\/.*$)/gm, style: 'color: #8b949e; font-style: italic;' },
    { regex: /(\/\*[\s\S]*?\*\/)/g, style: 'color: #8b949e; font-style: italic;' },
    // 关键字
    { regex: /\b(const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|new|this|class|extends|import|export|from|default|async|await|try|catch|finally|throw|typeof|instanceof)\b/g, style: 'color: #ff7b72;' },
    // 布尔值和 null
    { regex: /\b(true|false|null|undefined|NaN)\b/g, style: 'color: #79c0ff;' },
    // 数字
    { regex: /\b(\d+\.?\d*)\b/g, style: 'color: #79c0ff;' },
    // 函数调用
    { regex: /\b([a-zA-Z_]\w*)\s*(?=\()/g, style: 'color: #d2a8ff;' },
  ];

  // 应用高亮
  patterns.forEach(({ regex, style }) => {
    escaped = escaped.replace(regex, `<span style="${style}">$1</span>`);
  });

  return escaped;
};

export function SimpleCodeEditor({
  roomId,
  userId,
  userName,
  initialCode = '// 在这里开始编写代码...\n',
  onCodeChange,
  otherUsers = [],
}: CodeEditorProps) {
  const [code, setCode] = useState(initialCode);
  const [language, setLanguage] = useState('javascript');
  const [lineNumbers, setLineNumbers] = useState<number[]>([1]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const highlightRef = useRef<HTMLPreElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const isLocalChangeRef = useRef(false);

  // 更新行号
  useEffect(() => {
    const lines = code.split('\n').length;
    setLineNumbers(Array.from({ length: lines }, (_, i) => i + 1));
  }, [code]);

  // 同步滚动
  const handleScroll = () => {
    if (highlightRef.current && textareaRef.current) {
      highlightRef.current.scrollTop = textareaRef.current.scrollTop;
      highlightRef.current.scrollLeft = textareaRef.current.scrollLeft;
    }
  };

  // 处理代码变化
  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newCode = e.target.value;
    setCode(newCode);
    onCodeChange?.(newCode);
  };

  // 处制代码
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      message.success('代码已复制到剪贴板');
    } catch {
      message.error('复制失败');
    }
  };

  // 下载代码
  const handleDownload = () => {
    const extensions: Record<string, string> = {
      javascript: 'js',
      typescript: 'ts',
      python: 'py',
      html: 'html',
      css: 'css',
      json: 'json',
      markdown: 'md',
    };

    const ext = extensions[language] || 'txt';
    const blob = new Blob([code], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `code-${roomId}.${ext}`;
    link.click();
    URL.revokeObjectURL(url);
    message.success('代码已下载');
  };

  // 处理 Tab 键
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      const textarea = e.currentTarget;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;

      const newValue = code.substring(0, start) + '  ' + code.substring(end);
      setCode(newValue);
      onCodeChange?.(newValue);

      // 恢复光标位置
      setTimeout(() => {
        textarea.selectionStart = textarea.selectionEnd = start + 2;
      }, 0);
    }
  };

  return (
    <div className="h-full flex flex-col bg-slate-900">
      {/* 工具栏 */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-white/10 bg-black/20">
        <div className="flex items-center gap-3">
          <select
            value={language}
            onChange={(e) => setLanguage(e.target.value)}
            className="bg-white/5 border border-white/10 rounded px-3 py-1.5 text-sm text-white focus:outline-none focus:border-blue-500/50"
          >
            <option value="javascript">JavaScript</option>
            <option value="typescript">TypeScript</option>
            <option value="python">Python</option>
            <option value="html">HTML</option>
            <option value="css">CSS</option>
            <option value="json">JSON</option>
            <option value="markdown">Markdown</option>
          </select>

          {/* 在线协作者 */}
          {otherUsers.length > 0 && (
            <div className="flex items-center gap-1">
              {otherUsers.map((user) => (
                <div
                  key={user.id}
                  className="w-6 h-6 rounded-full flex items-center justify-center text-xs text-white font-medium"
                  style={{ backgroundColor: user.color }}
                  title={user.name}
                >
                  {user.name.charAt(0).toUpperCase()}
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={handleCopy}
            className="p-2 text-gray-400 hover:text-white transition-colors"
            title="复制代码"
          >
            <Copy size={16} />
          </button>
          <button
            onClick={handleDownload}
            className="p-2 text-gray-400 hover:text-white transition-colors"
            title="下载代码"
          >
            <Download size={16} />
          </button>
        </div>
      </div>

      {/* 编辑器 */}
      <div className="flex-1 overflow-hidden" ref={containerRef}>
        <div className="h-full flex">
          {/* 行号 */}
          <div className="py-3 px-2 bg-black/30 text-gray-500 text-sm text-right select-none border-r border-white/10 font-mono">
            {lineNumbers.map((num) => (
              <div key={num} className="leading-6">
                {num}
              </div>
            ))}
          </div>

          {/* 代码区域 */}
          <div className="flex-1 relative overflow-hidden">
            <pre
              ref={highlightRef}
              className="absolute inset-0 py-3 px-4 m-0 pointer-events-none overflow-auto font-mono text-sm leading-6 text-gray-300"
              style={{ whiteSpace: 'pre-wrap', wordWrap: 'break-word' }}
              dangerouslySetInnerHTML={{ __html: highlightSyntax(code, language) }}
            />
            <textarea
              ref={textareaRef}
              value={code}
              onChange={handleChange}
              onScroll={handleScroll}
              onKeyDown={handleKeyDown}
              className="absolute inset-0 py-3 px-4 w-full h-full bg-transparent text-transparent caret-white resize-none outline-none font-mono text-sm leading-6 overflow-auto"
              style={{ whiteSpace: 'pre-wrap', wordWrap: 'break-word' }}
              spellCheck={false}
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
            />
          </div>
        </div>
      </div>
    </div>
  );
}
