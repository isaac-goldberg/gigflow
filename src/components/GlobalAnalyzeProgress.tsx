import { useLibrary } from "../store/library";

export function GlobalAnalyzeProgress() {
  const { batchProgress, cancelAnalyzeAll, dismissBatchProgress } = useLibrary();
  if (!batchProgress) return null;

  const complete = batchProgress.done >= batchProgress.total;
  const pct = batchProgress.total > 0 ? Math.round((batchProgress.done / batchProgress.total) * 100) : 0;

  return (
    <div className="shrink-0 bg-gig-panel2 border-t border-gig-border px-4 py-1.5">
      <div className="flex items-center gap-3">
        <span className="text-xs font-bold uppercase tracking-wide text-gig-accent2 whitespace-nowrap">
          {complete
            ? batchProgress.cancelRequested
              ? `Stopped — ${batchProgress.done}/${batchProgress.total} analyzed`
              : `Done — ${batchProgress.done}/${batchProgress.total} analyzed`
            : `Analyzing ${batchProgress.done + 1} of ${batchProgress.total}`}
        </span>

        <div className="flex-1 h-1.5 bg-black rounded-sm overflow-hidden">
          <div className="h-full bg-gig-accent2" style={{ width: `${pct}%` }} />
        </div>

        {!complete && batchProgress.currentTitle && (
          <span className="text-[11px] text-gig-muted truncate max-w-xs">{batchProgress.currentTitle}</span>
        )}

        {!complete ? (
          <button
            onClick={cancelAnalyzeAll}
            className="text-[11px] font-bold uppercase text-gig-muted hover:text-gig-warn whitespace-nowrap"
          >
            Cancel
          </button>
        ) : (
          <button
            onClick={dismissBatchProgress}
            className="text-[11px] font-bold uppercase text-gig-muted hover:text-gig-text whitespace-nowrap"
          >
            Dismiss
          </button>
        )}
      </div>

      {batchProgress.failed.length > 0 && (
        <div className="mt-1 max-h-16 overflow-y-auto space-y-0.5">
          <div className="text-[11px] font-bold uppercase text-gig-warn">
            {batchProgress.failed.length} failed (skipped, rest continued):
          </div>
          {batchProgress.failed.map((f, i) => (
            <div key={i} className="text-[11px] text-gig-dim truncate" title={f.error}>
              {f.title} — {f.error}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
