/**
 * ApprovalCard — a single agent permission request awaiting approval.
 */
import type { ApprovalDecision, ApprovalRequest } from '../../stores/approvalStore';

interface Props {
  request: ApprovalRequest;
  onRespond: (decision: ApprovalDecision) => void;
}

export function ApprovalCard({ request, onRespond }: Props) {
  return (
    <div className="bg-bg-tertiary border border-border rounded-md p-2.5 space-y-2">
      <div>
        <p className="text-xs font-medium text-text-primary">{request.title}</p>
        {request.scope && (
          <p className="text-[10px] text-text-secondary mt-0.5">{request.scope}</p>
        )}
      </div>

      <div className="font-mono text-xs bg-bg-primary rounded px-2 py-1 text-text-primary overflow-x-auto whitespace-pre">
        <span className="text-text-secondary">$ </span>
        {request.command}
      </div>

      <div className="flex items-center gap-1.5">
        <button
          onClick={() => onRespond('allow')}
          className="px-2.5 py-1 text-xs rounded bg-green-700 text-white hover:bg-green-600 transition-colors"
        >
          允许
        </button>
        <button
          onClick={() => onRespond('allowAll')}
          className="px-2.5 py-1 text-xs rounded bg-bg-secondary border border-border text-text-secondary
                     hover:text-text-primary transition-colors"
        >
          全部
        </button>
        <button
          onClick={() => onRespond('reject')}
          className="px-2.5 py-1 text-xs rounded border border-red-700 text-red-400
                     hover:bg-red-900/30 transition-colors"
        >
          拒绝
        </button>
      </div>
    </div>
  );
}
