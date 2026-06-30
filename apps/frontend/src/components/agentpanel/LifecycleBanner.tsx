/**
 * LifecycleBanner — subtle centered divider for session lifecycle events.
 *
 * Used for session_created, turn_start, turn_end, and similar
 * non-conversation events that should be visually de-emphasised.
 */
import { Play, CheckCircle2 } from 'lucide-react';

interface Props {
  /** Human-readable label, e.g. "会话已创建" or "第 3 轮完成". */
  label: string;
  /** Controls the icon and colour tone. */
  variant?: 'start' | 'end' | 'info';
}

const VARIANT_STYLE: Record<NonNullable<Props['variant']>, string> = {
  start: 'text-green-500/70 border-green-500/30',
  end: 'text-text-secondary border-border',
  info: 'text-text-secondary border-border',
};

export function LifecycleBanner({ label, variant = 'info' }: Props) {
  const Icon = variant === 'start' ? Play : CheckCircle2;

  return (
    <div className="flex items-center gap-2 py-2">
      <div className={`flex-1 border-t ${VARIANT_STYLE[variant]}`} />
      <div className={`flex items-center gap-1 text-[10px] ${VARIANT_STYLE[variant]}`}>
        <Icon size={10} strokeWidth={1.5} />
        <span>{label}</span>
      </div>
      <div className={`flex-1 border-t ${VARIANT_STYLE[variant]}`} />
    </div>
  );
}
