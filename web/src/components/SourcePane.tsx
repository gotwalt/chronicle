import { useEffect, useState } from "react";
import { codeToHtml } from "shiki";
import type { FileAnnotation } from "../types";

function buildAnnotatedLineSet(annotations: FileAnnotation[]): Map<number, number[]> {
  // Maps line number -> array of annotation indices that cover that line
  const lineMap = new Map<number, number[]>();
  annotations.forEach((ann, idx) => {
    if (ann.lines) {
      for (let line = ann.lines.start; line <= ann.lines.end; line++) {
        const existing = lineMap.get(line);
        if (existing) {
          existing.push(idx);
        } else {
          lineMap.set(line, [idx]);
        }
      }
    }
  });
  return lineMap;
}

export function SourcePane({
  content,
  language,
  annotations,
  onRegionClick,
}: {
  content: string;
  language: string;
  annotations: FileAnnotation[];
  onRegionClick: (index: number) => void;
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
          // Fallback: plain text
          setHighlightedHtml("");
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [content, language]);

  const annotatedLines = buildAnnotatedLineSet(annotations);
  const lines = content.split("\n");

  // Parse highlighted HTML to extract per-line HTML
  const lineHtmls: string[] = [];
  if (highlightedHtml) {
    // Shiki wraps code in <pre><code>...\n...\n...</code></pre>
    // Each line is: <span class="line"><span style="...">token</span>...</span>
    // Lines are separated by real newlines inside <code>
    const codeMatch = highlightedHtml.match(/<code[^>]*>([\s\S]*)<\/code>/);
    if (codeMatch) {
      const rawLines = codeMatch[1].split("\n");
      for (const rawLine of rawLines) {
        // Strip the outer <span class="line">...</span> wrapper, keep inner token spans
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
            const annIndices = annotatedLines.get(lineNum);
            const isAnnotated = annIndices !== undefined && annIndices.length > 0;
            const lineHtml = lineHtmls[i];

            return (
              <tr
                key={i}
                className={`group ${isAnnotated ? "cursor-pointer hover:bg-zinc-800/60" : ""}`}
                onClick={
                  isAnnotated
                    ? () => onRegionClick(annIndices[0])
                    : undefined
                }
              >
                {/* Gutter marker */}
                <td className="w-1 select-none p-0">
                  {isAnnotated && (
                    <div className="h-full w-[3px] bg-emerald-500" />
                  )}
                </td>
                {/* Line number */}
                <td className="select-none pr-4 pl-3 text-right align-top text-zinc-600 leading-6">
                  {lineNum}
                </td>
                {/* Code */}
                <td className="whitespace-pre pr-4 leading-6">
                  {lineHtml ? (
                    <span dangerouslySetInnerHTML={{ __html: lineHtml }} />
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
