/**
 * ApprovalQueue — left-sidebar panel listing pending agent approval requests.
 */
import { useApprovalQueue } from '../../hooks/useApprovalQueue';
import { ApprovalCard } from './ApprovalCard';

export function ApprovalQueue() {
  const { pending, pendingCount, respond } = useApprovalQueue();
  const first = pending[0];

  return (
    <div className="flex flex-col border-t border-border">
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wider">
          审批流队列
        </span>
        {pendingCount > 0 && (
          <span className="min-w-[16px] h-4 px-1 flex items-center justify-center
                           bg-red-500 text-white text-[10px] font-bold rounded-full">
            {pendingCount}
          </span>
        )}
      </div>

      <div className="px-3 pb-3">
        {first ? (
          <>
            <ApprovalCard request={first} onRespond={(d) => respond(first.id, d)} />
            {pendingCount > 1 && (
              <p className="text-xs text-text-secondary mt-2">
                {pendingCount - 1} 其它待处理请求...
              </p>
            )}
          </>
        ) : (
          <p className="text-xs text-text-secondary italic">暂无待审批请求</p>
        )}
      </div>
    </div>
  );
}
