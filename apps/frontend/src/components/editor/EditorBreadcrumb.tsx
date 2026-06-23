/**
 * EditorBreadcrumb — file path + language + dirty/save status row.
 */
interface Props {
  path: string;
  language: string;
  dirty: boolean;
  state: 'loading' | 'ready' | 'saving' | 'error';
  onSave: () => void;
}

export function EditorBreadcrumb({ path, language, dirty, state, onSave }: Props) {
  return (
    <div className="flex items-center justify-between px-3 py-2 bg-bg-secondary border-b border-border flex-shrink-0">
      <div className="flex items-center gap-2 text-xs">
        <span
          className={`w-1.5 h-1.5 rounded-full ${
            state === 'error'
              ? 'bg-red-400'
              : dirty
              ? 'bg-yellow-400'
              : 'bg-green-400'
          }`}
        />
        <span className="text-text-secondary font-mono">{path}</span>
        <span className="text-text-secondary opacity-50">·</span>
        <span className="text-text-secondary">{language}</span>
        {dirty && <span className="text-yellow-400">●</span>}
      </div>
      <button
        onClick={onSave}
        disabled={!dirty || state === 'saving'}
        className="px-3 py-1 text-xs rounded bg-accent text-white
                   hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
      >
        {state === 'saving' ? '保存中...' : '保存 (Ctrl+S)'}
      </button>
    </div>
  );
}
