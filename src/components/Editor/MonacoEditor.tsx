/**
 * 协同代码编辑器组件 - 仅客户端渲染
 * 使用本地 Monaco Editor 文件，无需网络请求
 */

'use client';

import { useEffect, useRef, useState } from 'react';
import dynamic from 'next/dynamic';
import { Download, Copy } from 'lucide-react';
import { message } from 'antd';
import { loader } from '@monaco-editor/react';

// Monaco 类型定义
type IStandaloneCodeEditor = any;
type MonacoType = any;

// 配置使用本地 Monaco Editor 文件（仅在客户端）
if (typeof window !== 'undefined') {
  loader.config({
    paths: {
      vs: '/monaco-editor/min/vs'
    }
  });
}

// 动态导入Monaco Editor，禁用SSR
const Editor = dynamic(() => import('@monaco-editor/react'), {
  ssr: false,
  loading: () => (
    <div className="h-full flex items-center justify-center bg-slate-900">
      <div className="text-gray-400 text-sm">加载编辑器...</div>
    </div>
  )
});

// Monaco 编辑器光标和选择信息
interface EditorCursor {
  lineNumber: number;
  column: number;
}

interface EditorSelection {
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
}

// 其他用户的编辑器光标信息
interface OtherUserEditorCursor {
  userId: string;
  userName: string;
  color: string;
  cursor?: EditorCursor;
  selection?: EditorSelection;
}

interface CodeEditorProps {
  roomId: string;
  initialCode?: string;
  onCodeChange?: (code: string) => void;
  otherUsers?: Array<{ id: string; name: string; color: string; selection?: { from: number; to: number } }>;
  // 编辑器协同编辑相关回调
  onCursorChange?: (cursor: EditorCursor) => void;
  onSelectionChange?: (selection: EditorSelection) => void;
  otherEditorCursors?: Map<string, OtherUserEditorCursor>;
}

export function MonacoEditor({
  roomId,
  initialCode = '// 在这里开始编写代码...\n',
  onCodeChange,
  otherUsers = [],
  onCursorChange,
  onSelectionChange,
  otherEditorCursors,
}: CodeEditorProps) {
  const [code, setCode] = useState(initialCode);
  const [language, setLanguage] = useState('javascript');
  const editorRef = useRef<IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<MonacoType | null>(null);
  const isLocalChangeRef = useRef(false);
  const decorationsCollectionRef = useRef<any>(null); // 存储其他用户光标的装饰集合

  // 当 initialCode 从外部变化时（其他用户修改），更新编辑器
  useEffect(() => {
    if (!editorRef.current) return;

    // 如果是本地用户触发的变化，不更新
    if (isLocalChangeRef.current) {
      isLocalChangeRef.current = false;
      return;
    }

    const editor = editorRef.current;
    const currentValue = editor.getValue();

    // 只有当外部代码确实不同时才更新
    if (currentValue !== initialCode) {
      editor.setValue(initialCode);
      setCode(initialCode);
    }
  }, [initialCode]);

  const handleEditorMount = (
    editor: IStandaloneCodeEditor,
    monaco: MonacoType
  ) => {
    editorRef.current = editor;
    monacoRef.current = monaco;

    // 设置主题
    monaco.editor.defineTheme('strange-room-dark', {
      base: 'vs-dark',
      inherit: true,
      rules: [],
      colors: {
        'editor.background': '#0f172a',
        'editor.foreground': '#e2e8f0',
        'editorLineNumber.foreground': '#475569',
        'editorCursor.foreground': '#60a5fa',
        'editor.selectionBackground': '#1e40af40',
        'editor.inactiveSelectionBackground': '#1e40af20',
      },
    });
    monaco.editor.setTheme('strange-room-dark');

    // 配置编辑器选项
    editor.updateOptions({
      fontSize: 14,
      fontFamily: "'Consolas', 'Monaco', 'Courier New', monospace",
      fontLigatures: false,
      minimap: { enabled: true },
      scrollBeyondLastLine: false,
      automaticLayout: true,
      tabSize: 2,
      wordWrap: 'on',
      lineNumbers: 'on',
      renderLineHighlight: 'all',
      cursorBlinking: 'smooth',
      cursorSmoothCaretAnimation: 'on',
    });

    // 监听光标变化，同步到其他用户
    editor.onDidChangeCursorPosition((e: any) => {
      if (onCursorChange && e.position) {
        onCursorChange({
          lineNumber: e.position.lineNumber,
          column: e.position.column,
        });
      }
    });

    // 监听选择变化
    editor.onDidChangeCursorSelection((e: any) => {
      if (onSelectionChange && e.selection) {
        onSelectionChange({
          startLineNumber: e.selection.startLineNumber,
          startColumn: e.selection.startColumn,
          endLineNumber: e.selection.endLineNumber,
          endColumn: e.selection.endColumn,
        });
      }
    });

    // 创建 decorations 集合用于显示其他用户的光标
    decorationsCollectionRef.current = editor.createDecorationsCollection([]);
  };

  const handleCodeChange = (value: string | undefined) => {
    const newCode = value || '';
    isLocalChangeRef.current = true; // 标记为本地变化
    setCode(newCode);
    onCodeChange?.(newCode);
  };

  // 复制代码
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

  // 渲染其他用户的光标和选择区域
  const renderOtherUsersCursors = () => {
    if (!editorRef.current || !decorationsCollectionRef.current) return;

    const decorations: any[] = [];

    // 遍历其他用户的编辑器光标信息
    if (otherEditorCursors) {
      otherEditorCursors.forEach((userCursor) => {
        const { cursor, selection, userName, color } = userCursor;

        // 渲染光标位置
        if (cursor) {
          decorations.push({
            range: {
              startLineNumber: cursor.lineNumber,
              startColumn: cursor.column,
              endLineNumber: cursor.lineNumber,
              endColumn: cursor.column,
            },
            options: {
              className: `other-user-cursor-${userCursor.userId}`,
              stickiness: 1, // AlwaysGrowsWhenTypingAtEdges
              beforeContentClassName: `remote-cursor-before`,
              hoverMessage: { value: userName },
            },
          });
        }

        // 渲染选择区域
        if (selection) {
          decorations.push({
            range: {
              startLineNumber: selection.startLineNumber,
              startColumn: selection.startColumn,
              endLineNumber: selection.endLineNumber,
              endColumn: selection.endColumn,
            },
            options: {
              className: `other-user-selection`,
              backgroundColor: `${color}20`, // 添加透明度
              overviewRuler: {
                color: color,
                position: 4, // Right
              },
              hoverMessage: { value: `${userName} 正在选择` },
            },
          });
        }
      });
    }

    // 更新 decorations
    decorationsCollectionRef.current.set(decorations);
  };

  // 监听 otherEditorCursors 变化，更新显示
  useEffect(() => {
    renderOtherUsersCursors();
  }, [otherEditorCursors]);

  // 注入 CSS 样式用于其他用户光标
  useEffect(() => {
    if (typeof document === 'undefined') return;

    // 检查是否已存在样式
    if (document.getElementById('monaco-remote-cursors-style')) return;

    const style = document.createElement('style');
    style.id = 'monaco-remote-cursors-style';
    style.textContent = `
      .other-user-selection {
        opacity: 0.3;
      }
      .remote-cursor-before {
        position: absolute;
        width: 2px !important;
        height: 100% !important;
        animation: blink 1s infinite;
      }
      @keyframes blink {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.3; }
      }
    `;
    document.head.appendChild(style);

    return () => {
      style.remove();
    };
  }, []);

  return (
    <div className="h-full flex flex-col bg-slate-900">
      {/* 工具栏 */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-white/10 bg-black/20">
        <div className="flex items-center gap-3">
          <select
            value={language}
            onChange={(e) => setLanguage(e.target.value)}
            className="bg-slate-800 border border-white/10 rounded px-3 py-1.5 text-sm text-white focus:outline-none focus:border-blue-500/50"
          >
            <option value="javascript" style={{ backgroundColor: '#1e293b', color: 'white' }}>JavaScript</option>
            <option value="typescript" style={{ backgroundColor: '#1e293b', color: 'white' }}>TypeScript</option>
            <option value="python" style={{ backgroundColor: '#1e293b', color: 'white' }}>Python</option>
            <option value="html" style={{ backgroundColor: '#1e293b', color: 'white' }}>HTML</option>
            <option value="css" style={{ backgroundColor: '#1e293b', color: 'white' }}>CSS</option>
            <option value="json" style={{ backgroundColor: '#1e293b', color: 'white' }}>JSON</option>
            <option value="markdown" style={{ backgroundColor: '#1e293b', color: 'white' }}>Markdown</option>
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
      <div className="flex-1">
        <Editor
          height="100%"
          language={language}
          value={code}
          onChange={handleCodeChange}
          onMount={handleEditorMount}
          theme="vs-dark"
          options={{
            readOnly: false,
            domReadOnly: false,
          }}
        />
      </div>
    </div>
  );
}
