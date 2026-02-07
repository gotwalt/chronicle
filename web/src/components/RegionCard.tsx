import { useState } from "react";
import { useNavigate } from "react-router";
import type { FileAnnotation } from "../types";

function timeAgo(timestamp: string): string {
  const seconds = Math.floor(
    (Date.now() - new Date(timestamp).getTime()) / 1000
  );
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

export function RegionCard({
  annotation,
  isSelected,
  minHeight,
}: {
  annotation: FileAnnotation;
  isSelected: boolean;
  minHeight?: number;
}) {
  const [reasoningOpen, setReasoningOpen] = useState(false);
  const [rawOpen, setRawOpen] = useState(false);
  const navigate = useNavigate();

  const { anchor, lines } = annotation;
  const lineRange = lines ? `L${lines.start}-${lines.end}` : "";
  const header = `${anchor.unit_type} ${anchor.name}`;

  return (
    <div
      className={`flex flex-col rounded-lg border p-4 transition-colors ${
        isSelected
          ? "border-emerald-500 bg-zinc-800/80"
          : "border-zinc-800 bg-zinc-900 hover:border-zinc-700"
      }`}
      style={minHeight ? { minHeight } : undefined}
    >
      {/* Header */}
      <div className="mb-2 flex items-baseline justify-between gap-2">
        <span className="font-mono text-sm font-semibold text-zinc-200">
          {header}
        </span>
        {lineRange && (
          <span className="shrink-0 font-mono text-xs text-zinc-500">
            {lineRange}
          </span>
        )}
      </div>

      {/* Signature */}
      {anchor.signature && (
        <div className="mb-2 font-mono text-xs text-zinc-500 break-all">
          {anchor.signature}
        </div>
      )}

      {/* Intent */}
      <p className="mb-3 text-sm leading-relaxed text-zinc-300">
        {annotation.intent}
      </p>

      {/* Reasoning (collapsible) */}
      {annotation.reasoning && (
        <div className="mb-3">
          <button
            onClick={() => setReasoningOpen(!reasoningOpen)}
            className="flex items-center gap-1 text-xs font-medium text-zinc-400 hover:text-zinc-300 transition-colors cursor-pointer"
          >
            <span
              className={`inline-block transition-transform ${reasoningOpen ? "rotate-90" : ""}`}
            >
              ▸
            </span>
            Reasoning
          </button>
          {reasoningOpen && (
            <p className="mt-1.5 text-xs leading-relaxed text-zinc-400 pl-3 border-l border-zinc-700">
              {annotation.reasoning}
            </p>
          )}
        </div>
      )}

      {/* Constraints */}
      {annotation.constraints.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-1.5">
          {annotation.constraints.map((c, i) => (
            <span
              key={i}
              className="inline-block rounded-full bg-amber-500/10 px-2 py-0.5 text-xs text-amber-400 border border-amber-500/20"
              title={`Source: ${c.source}`}
            >
              {c.text}
            </span>
          ))}
        </div>
      )}

      {/* Dependencies */}
      {annotation.semantic_dependencies.length > 0 && (
        <div className="mb-3">
          <div className="mb-1 text-xs font-medium text-zinc-500">
            Dependencies
          </div>
          <ul className="space-y-1">
            {annotation.semantic_dependencies.map((dep, i) => (
              <li key={i} className="text-xs text-zinc-400 flex gap-1.5">
                <span className="text-zinc-600">→</span>
                <span>
                  <button
                    onClick={() => navigate(`/file/${dep.file}`)}
                    className="text-emerald-400 hover:text-emerald-300 hover:underline cursor-pointer"
                  >
                    {dep.file}
                  </button>
                  {dep.anchor && (
                    <span className="font-mono text-zinc-500">
                      :{dep.anchor}
                    </span>
                  )}
                  <span className="text-zinc-600 ml-1">({dep.nature})</span>
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Tags */}
      {annotation.tags.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-1.5">
          {annotation.tags.map((tag) => (
            <span
              key={tag}
              className="inline-block rounded-full bg-emerald-500/10 px-2 py-0.5 text-xs text-emerald-400 border border-emerald-500/20"
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      {/* Risk notes */}
      {annotation.risk_notes && (
        <div className="mb-3 rounded border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs text-amber-300">
          <span className="font-medium">Risk:</span> {annotation.risk_notes}
        </div>
      )}

      {/* Corrections */}
      {annotation.corrections.length > 0 && (
        <div className="mb-3 rounded border border-blue-500/30 bg-blue-500/5 px-3 py-2 text-xs text-blue-300">
          <span className="font-medium">
            {annotation.corrections.length} correction
            {annotation.corrections.length > 1 ? "s" : ""}
          </span>
          {annotation.corrections.map((c, i) => (
            <div key={i} className="mt-1 text-blue-400/80">
              {c.field}: {c.reason}
            </div>
          ))}
        </div>
      )}

      {/* Raw JSON (collapsible) */}
      <div className="mb-3">
        <button
          onClick={() => setRawOpen(!rawOpen)}
          className="flex items-center gap-1 text-xs font-medium text-zinc-500 hover:text-zinc-400 transition-colors cursor-pointer"
        >
          <span
            className={`inline-block transition-transform ${rawOpen ? "rotate-90" : ""}`}
          >
            ▸
          </span>
          Raw JSON
        </button>
        {rawOpen && (
          <pre className="mt-1.5 overflow-x-auto rounded border border-zinc-700 bg-zinc-950 p-3 text-[11px] leading-relaxed text-zinc-400 font-mono">
            {JSON.stringify(annotation, null, 2)}
          </pre>
        )}
      </div>

      {/* Footer */}
      <div className="flex items-center justify-between text-xs text-zinc-600">
        <span className="font-mono">{annotation.commit.slice(0, 7)}</span>
        <span>{timeAgo(annotation.timestamp)}</span>
      </div>
    </div>
  );
}
