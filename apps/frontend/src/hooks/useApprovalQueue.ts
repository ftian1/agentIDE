/**
 * useApprovalQueue — headless view-model for the approval flow queue.
 */
import { useApprovalStore } from '../stores/approvalStore';
import type { ApprovalDecision, ApprovalRequest } from '../stores/approvalStore';

export interface ApprovalQueueView {
  pending: ApprovalRequest[];
  pendingCount: number;
  respond: (id: string, decision: ApprovalDecision) => void;
}

export function useApprovalQueue(): ApprovalQueueView {
  const requests = useApprovalStore((s) => s.requests);
  const respond = useApprovalStore((s) => s.respond);

  const pending = Object.values(requests)
    .filter((r) => r.status === 'pending')
    .sort((a, b) => a.createdAt.localeCompare(b.createdAt));

  return { pending, pendingCount: pending.length, respond };
}
