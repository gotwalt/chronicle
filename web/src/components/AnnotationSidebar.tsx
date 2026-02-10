import { useState } from "react";
import type { LookupOutput, SummaryOutput } from "../types";

export function AnnotationSidebar({
  lookup,
  summary,
  onNavigateFile,
}: {
  lookup: LookupOutput;
  summary: SummaryOutput;
  onNavigateFile: (path: string) => void;
}) {
  const hasStale =
    lookup.staleness && lookup.staleness.some((s) => s.stale);

  return (
    <div className="p-4 space-y-4">
      {/* Staleness alert */}
      {hasStale && (
        <div className="rounded border border-amber-700 bg-amber-950/50 px-3 py-2 text-sm text-amber-300">
          Some annotations may be stale — the file has changed since they
          were written.
        </div>
      )}

      {/* Recent history */}
      {lookup.recent_history.length > 0 && (
        <Section title="Recent History">
          <div className="space-y-2">
            {lookup.recent_history.map((entry, i) => (
              <div key={i} className="text-sm">
                <div className="flex items-baseline gap-2">
                  <code className="text-xs text-zinc-500">
                    {entry.commit.slice(0, 7)}
                  </code>
                  <SchemaBadge schema={entry.original_schema} />
                  <span className="text-xs text-zinc-500">
                    {formatTimestamp(entry.timestamp)}
                  </span>
                </div>
                <p className="text-zinc-300 mt-0.5">{entry.intent}</p>
                {entry.constraints?.length > 0 && (
                  <div className="flex flex-wrap gap-1 mt-1">
                    {entry.constraints.map((c, j) => (
                      <span
                        key={j}
                        className="rounded bg-amber-900/50 px-1.5 py-0.5 text-xs text-amber-300"
                      >
                        {c}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Summary units */}
      {summary.units.length > 0 && (
        <Section title="Annotated Regions">
          <div className="space-y-2">
            {summary.units.map((unit, i) => (
              <div
                key={i}
                className="rounded border border-zinc-800 bg-zinc-900/50 p-2 text-sm"
              >
                <div className="flex items-baseline gap-2">
                  <span className="font-mono text-xs text-emerald-400">
                    {unit.anchor.type} {unit.anchor.name}
                  </span>
                  {unit.lines.start > 0 && (
                    <span className="text-xs text-zinc-500">
                      L{unit.lines.start}-{unit.lines.end}
                    </span>
                  )}
                </div>
                <p className="text-zinc-300 mt-1">{unit.intent}</p>
                {unit.constraints?.length > 0 && (
                  <div className="flex flex-wrap gap-1 mt-1">
                    {unit.constraints.map((c, j) => (
                      <span
                        key={j}
                        className="rounded bg-amber-900/50 px-1.5 py-0.5 text-xs text-amber-300"
                      >
                        {c}
                      </span>
                    ))}
                  </div>
                )}
                {unit.risk_notes && (
                  <p className="mt-1 text-xs text-orange-400">
                    {unit.risk_notes}
                  </p>
                )}
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Contracts */}
      {lookup.contracts.length > 0 && (
        <Section title="Contracts">
          <div className="space-y-1.5">
            {lookup.contracts.map((c, i) => (
              <div key={i} className="text-sm">
                <p className="text-zinc-300">{c.description}</p>
                <div className="flex gap-2 mt-0.5">
                  {c.anchor && (
                    <span className="text-xs text-zinc-500">
                      @ {c.anchor}
                    </span>
                  )}
                  <span className="text-xs text-zinc-600">
                    [{c.source}]
                  </span>
                </div>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Decisions */}
      {lookup.decisions.length > 0 && (
        <Section title="Decisions">
          <div className="space-y-2">
            {lookup.decisions.map((d, i) => (
              <div
                key={i}
                className="rounded border border-zinc-800 bg-zinc-900/50 p-2 text-sm"
              >
                <p className="font-medium text-zinc-200">{d.what}</p>
                <p className="text-zinc-400 mt-0.5">{d.why}</p>
                <div className="flex items-center gap-2 mt-1.5">
                  <StabilityBadge stability={d.stability} />
                  {d.revisit_when && (
                    <span className="text-xs text-zinc-500">
                      revisit: {d.revisit_when}
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Dependencies */}
      {lookup.dependencies.length > 0 && (
        <Section title="Dependencies">
          <div className="space-y-1">
            {lookup.dependencies.map((dep, i) => (
              <div key={i} className="text-sm">
                <button
                  onClick={() => onNavigateFile(dep.target_file)}
                  className="font-mono text-xs text-blue-400 hover:underline cursor-pointer"
                >
                  {dep.target_file}:{dep.target_anchor}
                </button>
                <p className="text-zinc-500 text-xs">{dep.assumption}</p>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Follow-ups */}
      {lookup.open_follow_ups.length > 0 && (
        <Section title="Open Follow-ups">
          <div className="space-y-1">
            {lookup.open_follow_ups.map((fu, i) => (
              <div key={i} className="text-sm flex gap-2">
                <code className="text-xs text-zinc-500 shrink-0">
                  {fu.commit.slice(0, 7)}
                </code>
                <span className="text-zinc-300">{fu.follow_up}</span>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Knowledge */}
      {lookup.knowledge && !isKnowledgeEmpty(lookup.knowledge) && (
        <Section title="Knowledge">
          {lookup.knowledge.conventions.length > 0 && (
            <div className="mb-2">
              <h4 className="text-xs font-medium text-zinc-500 mb-1">
                Conventions
              </h4>
              {lookup.knowledge.conventions.map((c, i) => (
                <p key={i} className="text-sm text-zinc-300 mb-1">
                  <span className="text-zinc-500">[{c.scope}]</span>{" "}
                  {c.rule}
                </p>
              ))}
            </div>
          )}
          {lookup.knowledge.boundaries.length > 0 && (
            <div className="mb-2">
              <h4 className="text-xs font-medium text-zinc-500 mb-1">
                Boundaries
              </h4>
              {lookup.knowledge.boundaries.map((b, i) => (
                <p key={i} className="text-sm text-zinc-300 mb-1">
                  <span className="font-mono text-xs text-zinc-500">
                    {b.module}
                  </span>{" "}
                  {b.boundary}
                </p>
              ))}
            </div>
          )}
          {lookup.knowledge.anti_patterns.length > 0 && (
            <div>
              <h4 className="text-xs font-medium text-zinc-500 mb-1">
                Anti-patterns
              </h4>
              {lookup.knowledge.anti_patterns.map((a, i) => (
                <div key={i} className="text-sm mb-1">
                  <p className="text-red-400">{a.pattern}</p>
                  <p className="text-zinc-400 text-xs">
                    Instead: {a.instead}
                  </p>
                </div>
              ))}
            </div>
          )}
        </Section>
      )}
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(true);
  return (
    <div>
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-zinc-500 hover:text-zinc-400 cursor-pointer mb-2"
      >
        <span
          className={`text-[10px] transition-transform ${open ? "rotate-90" : ""}`}
        >
          ▸
        </span>
        {title}
      </button>
      {open && children}
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

function isKnowledgeEmpty(k: {
  conventions: unknown[];
  boundaries: unknown[];
  anti_patterns: unknown[];
}): boolean {
  return (
    k.conventions.length === 0 &&
    k.boundaries.length === 0 &&
    k.anti_patterns.length === 0
  );
}

function SchemaBadge({ schema }: { schema?: string }) {
  if (!schema) return null;
  const short = schema.replace("chronicle/", "");
  const colors: Record<string, string> = {
    v1: "bg-zinc-800 text-zinc-500",
    v2: "bg-zinc-800 text-zinc-400",
    v3: "bg-emerald-900/50 text-emerald-400",
  };
  return (
    <span
      className={`rounded px-1 py-0.5 text-[10px] font-mono ${colors[short] ?? "bg-zinc-800 text-zinc-500"}`}
    >
      {short}
    </span>
  );
}

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 30) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  } catch {
    return ts;
  }
}
