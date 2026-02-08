import { useEffect, useState } from "react";
import { codeToHtml } from "shiki";
import type { SummaryUnit } from "../types";

/** Map marker kind keywords to gutter colors */
function gutterColor(unit: SummaryUnit): string {
  const notes = (unit.risk_notes ?? "").toUpperCase();
  if (notes.includes("SECURITY")) return "bg-rose-500";
  if (notes.includes("DEPRECATED")) return "bg-zinc-500";
  if (notes.includes("TECH_DEBT")) return "bg-orange-500";
  if (notes.includes("UNSTABLE")) return "bg-orange-400";
  if (notes.includes("PERF")) return "bg-purple-500";
  if (notes.includes("TEST_COVERAGE")) return "bg-green-500";
  if (unit.constraints && unit.constraints.length > 0) return "bg-amber-500";
  return "bg-emerald-500";
}

function buildAnnotatedLineMap(
  units: SummaryUnit[]
): Map<number, { color: string }> {
  const lineMap = new Map<number, { color: string }>();
  for (const unit of units) {
    if (unit.lines.start === 0 && unit.lines.end === 0) continue;
    const color = gutterColor(unit);
    for (let line = unit.lines.start; line <= unit.lines.end; line++) {
      // First unit to claim a line wins
      if (!lineMap.has(line)) {
        lineMap.set(line, { color });
      }
    }
  }
  return lineMap;
}

export function SourcePane({
  content,
  language,
  units,
}: {
  content: string;
  language: string;
  units: SummaryUnit[];
}) {
  const [highlightedHtml, setHighlightedHtml] = useState<string>("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);

    codeToHtml(content, {
      lang: language || "text",
      theme: "github-dark",
    })
      .then((html) => {
        if (!cancelled) {
          setHighlightedHtml(html);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setHighlightedHtml("");
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [content, language]);

  const annotatedLines = buildAnnotatedLineMap(units);
  const lines = content.split("\n");

  // Parse highlighted HTML to extract per-line HTML
  const lineHtmls: string[] = [];
  if (highlightedHtml) {
    const codeMatch = highlightedHtml.match(/<code[^>]*>([\s\S]*)<\/code>/);
    if (codeMatch) {
      const rawLines = codeMatch[1].split("\n");
      for (const rawLine of rawLines) {
        const stripped = rawLine
          .replace(/^<span class="line">/, "")
          .replace(/<\/span>$/, "");
        lineHtmls.push(stripped);
      }
    }
  }

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-zinc-500">
        Loading...
      </div>
    );
  }

  return (
    <div className="font-mono text-sm">
      <table className="w-full border-collapse">
        <tbody>
          {lines.map((line, i) => {
            const lineNum = i + 1;
            const marker = annotatedLines.get(lineNum);

            return (
              <tr key={i} className="group">
                {/* Gutter marker */}
                <td className="w-1 select-none p-0">
                  {marker && (
                    <div className={`h-full w-[3px] ${marker.color}`} />
                  )}
                </td>
                {/* Line number */}
                <td className="select-none pr-4 pl-3 text-right align-top text-zinc-600 leading-6">
                  {lineNum}
                </td>
                {/* Code */}
                <td className="whitespace-pre pr-4 leading-6">
                  {lineHtmls[i] ? (
                    <span
                      dangerouslySetInnerHTML={{ __html: lineHtmls[i] }}
                    />
                  ) : (
                    <span className="text-zinc-300">{line}</span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
