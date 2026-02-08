import { useEffect, useState } from "react";
import type { KnowledgeStore } from "../types";
import { fetchKnowledge } from "../api";

export function KnowledgePage() {
  const [data, setData] = useState<KnowledgeStore | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchKnowledge()
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
        Loading knowledge...
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

  const isEmpty =
    data.conventions.length === 0 &&
    data.boundaries.length === 0 &&
    data.anti_patterns.length === 0;

  return (
    <div className="h-full overflow-auto p-6">
      <div className="max-w-3xl mx-auto space-y-6">
        <div>
          <h2 className="text-lg font-semibold text-zinc-100">
            Knowledge Base
          </h2>
          <p className="text-sm text-zinc-500 mt-1">
            Conventions, module boundaries, and anti-patterns for this
            repository.
          </p>
        </div>

        {isEmpty && (
          <p className="text-sm text-zinc-500">
            No knowledge entries recorded yet. Use{" "}
            <code className="text-zinc-400">
              git chronicle knowledge add
            </code>{" "}
            to add entries.
          </p>
        )}

        {/* Conventions */}
        {data.conventions.length > 0 && (
          <div>
            <h3 className="text-sm font-medium text-zinc-400 mb-3">
              Conventions
            </h3>
            <div className="space-y-2">
              {data.conventions.map((c) => (
                <div
                  key={c.id}
                  className="rounded border border-zinc-800 bg-zinc-900/50 p-3"
                >
                  <p className="text-sm text-zinc-200">{c.rule}</p>
                  <div className="flex items-center gap-2 mt-1.5">
                    <span className="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-xs text-zinc-400">
                      {c.scope}
                    </span>
                    <StabilityBadge stability={c.stability} />
                    <span className="text-xs text-zinc-600">{c.id}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Boundaries */}
        {data.boundaries.length > 0 && (
          <div>
            <h3 className="text-sm font-medium text-zinc-400 mb-3">
              Module Boundaries
            </h3>
            <div className="space-y-2">
              {data.boundaries.map((b) => (
                <div
                  key={b.id}
                  className="rounded border border-zinc-800 bg-zinc-900/50 p-3"
                >
                  <div className="flex items-baseline gap-2">
                    <span className="font-mono text-sm text-zinc-300">
                      {b.module}
                    </span>
                  </div>
                  <p className="text-sm text-zinc-400 mt-1">
                    Owns: {b.owns}
                  </p>
                  <p className="text-sm text-zinc-400">
                    Boundary: {b.boundary}
                  </p>
                  <span className="text-xs text-zinc-600 mt-1 block">
                    {b.id}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Anti-patterns */}
        {data.anti_patterns.length > 0 && (
          <div>
            <h3 className="text-sm font-medium text-zinc-400 mb-3">
              Anti-patterns
            </h3>
            <div className="space-y-2">
              {data.anti_patterns.map((a) => (
                <div
                  key={a.id}
                  className="rounded border border-zinc-800 bg-zinc-900/50 p-3"
                >
                  <p className="text-sm text-red-400">{a.pattern}</p>
                  <p className="text-sm text-zinc-400 mt-1">
                    Instead: {a.instead}
                  </p>
                  <div className="flex items-center gap-2 mt-1.5">
                    <span className="text-xs text-zinc-600">{a.id}</span>
                    {a.learned_from && (
                      <span className="text-xs text-zinc-600">
                        from: {a.learned_from}
                      </span>
                    )}
                  </div>
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
      className={`rounded px-1.5 py-0.5 text-xs ${colors[stability] ?? "bg-zinc-800 text-zinc-400"}`}
    >
      {stability}
    </span>
  );
}
