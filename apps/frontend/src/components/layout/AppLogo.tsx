/**
 * AppLogo — the Remote AI IDE logo mark.
 *
 * Design: a hexagonal agent-core with an embedded prompt-cursor (>),
 * representing the fusion of structured AI agency and code execution.
 * The hexagon symbolizes the agent's multi-tool orchestration; the
 * cursor is the universal symbol of the agent/CLI prompt.
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
      {/* Outer glow ring */}
      <circle
        cx="64"
        cy="64"
        r="60"
        stroke="currentColor"
        strokeWidth="1"
        opacity="0.12"
      />

      {/* Hexagon frame — the agent structure */}
      {/* Points for regular hexagon centered at (64,64), radius 48 */}
      <polygon
        points="64,16 105.6,40 105.6,88 64,112 22.4,88 22.4,40"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinejoin="round"
        opacity="0.7"
      />

      {/* Inner hexagon — accent */}
      <polygon
        points="64,26 97.3,45.2 97.3,82.8 64,102 30.7,82.8 30.7,45.2"
        fill="none"
        stroke="currentColor"
        strokeWidth="1"
        strokeLinejoin="round"
        opacity="0.25"
      />

      {/* Prompt cursor: ">" — the universal agent/CLI symbol */}
      <text
        x="64"
        y="76"
        textAnchor="middle"
        fontSize="44"
        fontWeight="700"
        fill="currentColor"
        fontFamily="monospace"
        opacity="0.9"
      >
        &gt;
      </text>

      {/* Small accent dot — the agent "node" */}
      <circle cx="48" cy="56" r="3.5" fill="currentColor" opacity="0.6" />
      <circle cx="80" cy="56" r="3.5" fill="currentColor" opacity="0.6" />
    </svg>
  );
}
