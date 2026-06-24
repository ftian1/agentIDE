/**
 * CodeEditor — editable Monaco editor for a remote file buffer.
 *
 * Ctrl+S saves the draft back to the remote host.
 */
import { useEffect } from 'react';
import Editor from '@monaco-editor/react';
import { useFileBuffer } from '../../hooks/useFileBuffer';
import { detectLanguage } from '../../lib/languageDetector';
import { EditorBreadcrumb } from './EditorBreadcrumb';

interface Props {
  connectionId: string | null;
  path: string | null;
}

export function CodeEditor({ connectionId, path }: Props) {
  const { buffer, dirty, edit, save } = useFileBuffer(connectionId, path);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && (e.key === 's' || e.key === 'S')) {
        e.preventDefault();
        if (dirty) save();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [dirty, save]);

  if (!connectionId || !path) {
    return (
      <div className="flex-1 flex items-center justify-center bg-bg-primary">
        <p className="text-text-secondary text-sm">No file open.</p>
      </div>
    );
  }

  if (!buffer || buffer.state === 'loading') {
    return (
      <div className="flex-1 flex items-center justify-center bg-bg-primary">
        <p className="text-text-secondary text-sm">Loading {path}...</p>
      </div>
    );
  }

  if (buffer.state === 'error') {
    return (
      <div className="flex-1 flex items-center justify-center bg-bg-primary">
        <div className="text-center space-y-2">
          <p className="text-red-400 text-sm">Failed to open {path}</p>
          <p className="text-text-secondary text-xs">{buffer.error}</p>
        </div>
      </div>
    );
  }

  const lang = detectLanguage(path);

  return (
    <div className="flex flex-col h-full bg-bg-primary">
      <EditorBreadcrumb
        path={path}
        language={lang}
        dirty={dirty}
        state={buffer.state}
        onSave={save}
      />
      <div className="flex-1 overflow-hidden">
        <Editor
          height="100%"
          language={lang}
          theme="vs-dark"
          value={buffer.draft}
          onChange={(v) => edit(v ?? '')}
          options={{
            minimap: { enabled: false },
            lineNumbers: 'on',
            scrollBeyondLastLine: false,
            fontSize: 13,
            fontFamily: "Consolas, 'Cascadia Mono', 'Courier New', monospace",
            padding: { top: 8, bottom: 8 },
          }}
          loading={
            <div className="flex items-center justify-center h-full">
              <p className="text-text-secondary text-sm">Loading editor...</p>
            </div>
          }
        />
      </div>
    </div>
  );
}
