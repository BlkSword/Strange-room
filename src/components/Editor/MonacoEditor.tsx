/**
 * 协同代码编辑器组件 - 仅客户端渲染
 */

'use client';

import { useEffect, useRef, useState } from 'react';
import dynamic from 'next/dynamic';
import { Download, Copy } from 'lucide-react';
import { message } from 'antd';
import type Monaco from '@monaco-editor/react';

// Monaco 类型定义
type editor = Monaco.editor;
type IStandaloneCodeEditor = editor.IStandaloneCodeEditor;

// 动态导入Monaco Editor，禁用SSR
const Editor = dynamic(() => import('@monaco-editor/react'), {
  ssr: false,
  loading: () => (
    <div className="h-full flex items-center justify-center bg-slate-900">
      <div className="text-gray-400 text-sm">加载编辑器...</div>
    </div>
  )
});

interface CodeEditorProps {
  roomId: string;
  userId: string;
  userName: string;
  initialCode?: string;
  onCodeChange?: (code: string) => void;
  otherUsers?: Array<{ id: string; name: string; color: string; selection?: { from: number; to: number } }>;
}

export function MonacoEditor({
  roomId,
  userId,
  userName,
  initialCode = '// 在这里开始编写代码...\n',
  onCodeChange,
  otherUsers = [],
}: CodeEditorProps) {
  const [code, setCode] = useState(initialCode);
  const [language, setLanguage] = useState('javascript');
  const editorRef = useRef<IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof Monaco | null>(null);
  const isLocalChangeRef = useRef(false);

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
    monaco: typeof Monaco
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
      fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
      fontLigatures: true,
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
    editor.onDidChangeCursorPosition((e) => {
      // TODO: 通过 Yjs 同步光标位置
      console.log('Cursor position:', e.position);
    });

    // 监听选择变化
    editor.onDidChangeCursorSelection((e) => {
      // TODO: 通过 Yjs 同步选择区域
      console.log('Selection:', e.selection);
    });
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

  // 渲染其他用户的光标
  const renderOtherUsersCursors = () => {
    if (!editorRef.current) return null;

    const decorations: editor.IModelDeltaDecoration[] = [];

    otherUsers.forEach((user) => {
      if (user.selection) {
        decorations.push({
          range: {
            startLineNumber: 1,
            startColumn: user.selection.from,
            endLineNumber: 1,
            endColumn: user.selection.to,
          },
          options: {
            className: `other-user-selection-${user.id}`,
            hoverMessage: { value: user.name },
          },
        });
      }
    });

    // TODO: 使用 Monaco 的 decorations API 渲染其他用户的光标
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
