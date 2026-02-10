import { useEffect, useState } from "react";
import type { StatusOutput, TreeFile, RecentAnnotation } from "../types";
import { fetchRecentAnnotations } from "../api";

export function OverviewPage({
  status,
  files,
  onSelectFile,
}: {
  status: StatusOutput | null;
  files: TreeFile[];
  onSelectFile: (path: string) => void;
}) {
  const [annotations, setAnnotations] = useState<RecentAnnotation[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchRecentAnnotations()
      .then(setAnnotations)
      .catch((err) => {
        console.error("Failed to fetch recent annotations:", err);
        setAnnotations([]);
      })
      .finally(() => setLoading(false));
  }, []);

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

        {/* Recent Annotations (primary section) */}
        <div className="rounded border border-zinc-800 bg-zinc-900/50 p-4">
          <h3 className="text-sm font-medium text-zinc-400 mb-3">
            Recent Annotations
          </h3>
          {loading ? (
            <p className="text-xs text-zinc-600">Loading...</p>
          ) : annotations.length === 0 ? (
            <p className="text-xs text-zinc-600">No annotations yet.</p>
          ) : (
            <div className="space-y-3">
              {annotations.map((a) => (
                <div key={a.commit} className="rounded px-3 py-2">
                  <div className="flex items-center gap-2">
                    <code className="text-xs text-emerald-500 font-mono shrink-0">
                      {a.commit.slice(0, 8)}
                    </code>
                    <span className="text-sm text-zinc-300 truncate">
                      {a.message.split("\n")[0]}
                    </span>
                    <span className="text-xs text-zinc-600 shrink-0 ml-auto">
                      {formatRelativeTime(a.timestamp)}
                    </span>
                  </div>
                  <p className="text-xs text-zinc-500 mt-1">
                    {truncate(a.summary, 180)}
                  </p>
                  {a.files.length > 0 && (
                    <div className="flex flex-wrap gap-1 mt-1.5">
                      {a.files.map((f) => (
                        <button
                          key={f}
                          onClick={() => onSelectFile(f)}
                          className="text-[11px] font-mono text-zinc-500 hover:text-emerald-400 bg-zinc-800/60 hover:bg-zinc-800 rounded px-1.5 py-0.5 transition-colors cursor-pointer"
                        >
                          {f}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

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

        {/* Unannotated commits (demoted) */}
        {status && status.unannotated_commits.length > 0 && (
          <div className="rounded border border-zinc-800/60 bg-zinc-900/30 p-4">
            <h3 className="text-xs font-medium text-zinc-600 mb-2">
              Unannotated Commits
            </h3>
            <div className="space-y-1">
              {status.unannotated_commits.slice(0, 5).map((sha) => (
                <code
                  key={sha}
                  className="block text-xs text-zinc-600 font-mono"
                >
                  {sha.slice(0, 12)}
                </code>
              ))}
              {status.unannotated_commits.length > 5 && (
                <span className="text-xs text-zinc-700">
                  +{status.unannotated_commits.length - 5} more
                </span>
              )}
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

function truncate(text: string, max: number): string {
  // Collapse to single line, then cap length
  const oneline = text.replace(/\n+/g, " ").replace(/\s+/g, " ").trim();
  if (oneline.length <= max) return oneline;
  return oneline.slice(0, max).trimEnd() + "...";
}

function formatRelativeTime(timestamp: string): string {
  const now = Date.now();
  const then = new Date(timestamp).getTime();
  const seconds = Math.floor((now - then) / 1000);

  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}
