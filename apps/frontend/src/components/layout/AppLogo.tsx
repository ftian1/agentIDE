/**
 * AppLogo — the Remote AI IDE logo mark.
 *
 * Design: a hexagonal agent-core with an embedded prompt-cursor (>),
 * representing the fusion of structured AI agency and code execution.
 * Uses all-path rendering (no font dependency) for consistent display
 * across all platforms including Tauri webviews.
 */

interface Props {
  size?: number;
  className?: string;
}

export function AppLogo({ size = 72, className }: Props) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 128 128"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      {/* Outer ring */}
      <circle cx="64" cy="64" r="60" stroke="currentColor" strokeWidth="1" opacity="0.12" />
      {/* Outer hexagon */}
      <polygon
        points="64,16 105.6,40 105.6,88 64,112 22.4,88 22.4,40"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinejoin="round"
        opacity="0.7"
      />
      {/* Inner hexagon */}
      <polygon
        points="64,26 97.3,45.2 97.3,82.8 64,102 30.7,82.8 30.7,45.2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1"
        strokeLinejoin="round"
        opacity="0.25"
      />
      {/* ">" cursor as filled path (no font) */}
      <path d="M44,44 L80,64 L44,84 Z" fill="currentColor" opacity="0.85" />
      {/* Accent dots */}
      <circle cx="40" cy="64" r="3.5" fill="currentColor" opacity="0.5" />
      <circle cx="88" cy="64" r="3.5" fill="currentColor" opacity="0.5" />
    </svg>
  );
}
