import type { StatusOutput, TreeFile } from "../types";

export function OverviewPage({
  status,
  files,
  onSelectFile,
}: {
  status: StatusOutput | null;
  files: TreeFile[];
  onSelectFile: (path: string) => void;
}) {
  const annotatedFiles = files.filter((f) => f.annotation_count > 0);

  return (
    <div className="h-full overflow-auto p-6">
      <div className="max-w-2xl mx-auto space-y-6">
        <div>
          <h2 className="text-lg font-semibold text-zinc-100">
            Annotation Overview
          </h2>
          <p className="text-sm text-zinc-500 mt-1">
            Browse annotations stored alongside commits as structured metadata.
          </p>
        </div>

        {/* Stats cards */}
        {status && (
          <div className="grid grid-cols-3 gap-3">
            <StatCard
              label="Total Annotations"
              value={status.total_annotations}
            />
            <StatCard
              label="Coverage"
              value={`${status.coverage_pct}%`}
              sub={`${status.recent_annotated}/${status.recent_commits} recent`}
            />
            <StatCard
              label="Files with Annotations"
              value={annotatedFiles.length}
              sub={`of ${files.length} total`}
            />
          </div>
        )}

        {/* Unannotated commits */}
        {status && status.unannotated_commits.length > 0 && (
          <div className="rounded border border-zinc-800 bg-zinc-900/50 p-4">
            <h3 className="text-sm font-medium text-zinc-400 mb-2">
              Recent Unannotated Commits
            </h3>
            <div className="space-y-1">
              {status.unannotated_commits.slice(0, 5).map((sha) => (
                <code
                  key={sha}
                  className="block text-xs text-zinc-500 font-mono"
                >
                  {sha.slice(0, 12)}
                </code>
              ))}
              {status.unannotated_commits.length > 5 && (
                <span className="text-xs text-zinc-600">
                  +{status.unannotated_commits.length - 5} more
                </span>
              )}
            </div>
          </div>
        )}

        {/* Top annotated files */}
        {annotatedFiles.length > 0 && (
          <div className="rounded border border-zinc-800 bg-zinc-900/50 p-4">
            <h3 className="text-sm font-medium text-zinc-400 mb-2">
              Most Annotated Files
            </h3>
            <div className="space-y-1">
              {annotatedFiles
                .sort((a, b) => b.annotation_count - a.annotation_count)
                .slice(0, 10)
                .map((f) => (
                  <button
                    key={f.path}
                    onClick={() => onSelectFile(f.path)}
                    className="flex w-full items-center justify-between rounded px-2 py-1 text-sm hover:bg-zinc-800/50 cursor-pointer transition-colors"
                  >
                    <span className="font-mono text-xs text-zinc-300 truncate">
                      {f.path}
                    </span>
                    <span className="text-xs text-emerald-500 shrink-0 ml-2">
                      {f.annotation_count}
                    </span>
                  </button>
                ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function StatCard({
  label,
  value,
  sub,
}: {
  label: string;
  value: number | string;
  sub?: string;
}) {
  return (
    <div className="rounded border border-zinc-800 bg-zinc-900/50 p-3">
      <div className="text-2xl font-semibold text-zinc-100">{value}</div>
      <div className="text-xs text-zinc-500 mt-1">{label}</div>
      {sub && <div className="text-xs text-zinc-600">{sub}</div>}
    </div>
  );
}
