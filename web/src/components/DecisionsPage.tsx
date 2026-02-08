import { useEffect, useState } from "react";
import type { DecisionsOutput } from "../types";
import { fetchDecisions } from "../api";

export function DecisionsPage() {
  const [data, setData] = useState<DecisionsOutput | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchDecisions()
      .then((d) => {
        setData(d);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message);
        setLoading(false);
      });
  }, []);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-zinc-500">
        Loading decisions...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-red-400">
        {error}
      </div>
    );
  }

  if (!data) return null;

  const hasDecisions = data.decisions.length > 0;
  const hasRejected = data.rejected_alternatives.length > 0;

  return (
    <div className="h-full overflow-auto p-6">
      <div className="max-w-3xl mx-auto space-y-6">
        <div>
          <h2 className="text-lg font-semibold text-zinc-100">
            Design Decisions
          </h2>
          <p className="text-sm text-zinc-500 mt-1">
            Decisions recorded across all annotations in this repository.
          </p>
        </div>

        {!hasDecisions && !hasRejected && (
          <p className="text-sm text-zinc-500">
            No decisions recorded yet.
          </p>
        )}

        {hasDecisions && (
          <div className="space-y-3">
            {data.decisions.map((d, i) => (
              <div
                key={i}
                className="rounded border border-zinc-800 bg-zinc-900/50 p-4"
              >
                <div className="flex items-start justify-between gap-3">
                  <h3 className="text-sm font-medium text-zinc-200">
                    {d.what}
                  </h3>
                  <StabilityBadge stability={d.stability} />
                </div>
                <p className="text-sm text-zinc-400 mt-1">{d.why}</p>
                {d.revisit_when && (
                  <p className="text-xs text-zinc-500 mt-2">
                    Revisit when: {d.revisit_when}
                  </p>
                )}
                {d.scope.length > 0 && (
                  <div className="flex flex-wrap gap-1 mt-2">
                    {d.scope.map((s, j) => (
                      <span
                        key={j}
                        className="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-xs text-zinc-400"
                      >
                        {s}
                      </span>
                    ))}
                  </div>
                )}
                <div className="flex gap-2 mt-2 text-xs text-zinc-600">
                  <code>{d.commit.slice(0, 7)}</code>
                  <span>{d.timestamp}</span>
                </div>
              </div>
            ))}
          </div>
        )}

        {hasRejected && (
          <div>
            <h3 className="text-sm font-medium text-zinc-400 mb-3">
              Rejected Alternatives
            </h3>
            <div className="space-y-2">
              {data.rejected_alternatives.map((r, i) => (
                <div
                  key={i}
                  className="rounded border border-zinc-800 bg-zinc-900/30 p-3"
                >
                  <p className="text-sm text-zinc-300">{r.approach}</p>
                  <p className="text-sm text-zinc-500 mt-0.5">
                    Reason: {r.reason}
                  </p>
                  <code className="text-xs text-zinc-600 mt-1 block">
                    {r.commit.slice(0, 7)}
                  </code>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function StabilityBadge({ stability }: { stability: string }) {
  const colors: Record<string, string> = {
    permanent: "bg-green-900/60 text-green-400",
    provisional: "bg-yellow-900/60 text-yellow-400",
    experimental: "bg-red-900/60 text-red-400",
  };
  return (
    <span
      className={`shrink-0 rounded px-1.5 py-0.5 text-xs ${colors[stability] ?? "bg-zinc-800 text-zinc-400"}`}
    >
      {stability}
    </span>
  );
}
