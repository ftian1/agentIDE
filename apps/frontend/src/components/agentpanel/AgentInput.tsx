/**
 * AgentInput — agent prompt textarea + action toolbar.
 */
import { useState } from 'react';
import { AtSign, FolderPlus, Sparkles, ClipboardList, Send } from 'lucide-react';

interface Props {
  onSend: (text: string) => void;
  disabled?: boolean;
}

export function AgentInput({ onSend, disabled }: Props) {
  const [text, setText] = useState('');

  const submit = () => {
    const trimmed = text.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setText('');
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div className="border-t border-border p-2 space-y-2">
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
        rows={2}
        placeholder="向 Agent 提问, 或用 @提及子 Agent, 用 /调用技能..."
        className="w-full resize-none bg-bg-tertiary text-text-primary text-sm px-2 py-1.5 rounded
                   border border-border focus:outline-none focus:border-accent
                   placeholder:text-text-secondary"
      />
      <div className="flex items-center gap-1">
        <button className="flex items-center gap-1 px-1.5 py-1 text-xs text-text-secondary hover:text-text-primary rounded transition-colors">
          <AtSign size={12} strokeWidth={1.5} /> 提及
        </button>
        <button className="flex items-center gap-1 px-1.5 py-1 text-xs text-text-secondary hover:text-text-primary rounded transition-colors">
          <FolderPlus size={12} strokeWidth={1.5} /> 添加目录
        </button>
        <button className="flex items-center gap-1 px-1.5 py-1 text-xs text-text-secondary hover:text-text-primary rounded transition-colors">
          <Sparkles size={12} strokeWidth={1.5} /> 技能
        </button>
        <button className="flex items-center gap-1 px-1.5 py-1 text-xs text-text-secondary hover:text-text-primary rounded transition-colors">
          <ClipboardList size={12} strokeWidth={1.5} /> Plan
        </button>
        <div className="flex-1" />
        <button
          onClick={submit}
          disabled={disabled || !text.trim()}
          className="flex items-center gap-1 px-3 py-1 text-xs rounded bg-accent text-white
                     hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          <Send size={12} strokeWidth={1.5} /> 发送
        </button>
      </div>
    </div>
  );
}
