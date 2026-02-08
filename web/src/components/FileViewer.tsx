import type { FileViewResponse } from "../types";
import { SourcePane } from "./SourcePane";
import { AnnotationSidebar } from "./AnnotationSidebar";

export function FileViewer({
  data,
  onNavigateFile,
}: {
  data: FileViewResponse;
  onNavigateFile: (path: string) => void;
}) {
  const hasAnnotations =
    data.lookup.contracts.length > 0 ||
    data.lookup.decisions.length > 0 ||
    data.lookup.recent_history.length > 0 ||
    data.lookup.open_follow_ups.length > 0 ||
    data.summary.units.length > 0 ||
    (data.lookup.staleness && data.lookup.staleness.length > 0) ||
    (data.lookup.knowledge &&
      (data.lookup.knowledge.conventions.length > 0 ||
        data.lookup.knowledge.boundaries.length > 0 ||
        data.lookup.knowledge.anti_patterns.length > 0));

  return (
    <div className="flex h-full min-h-0">
      {/* Source code */}
      <div
        className={`min-w-0 overflow-auto border-r border-zinc-800 ${hasAnnotations ? "flex-[60]" : "flex-1"}`}
      >
        <SourcePane
          content={data.content}
          language={data.language}
          units={data.summary.units}
        />
      </div>
      {/* Annotation sidebar */}
      {hasAnnotations && (
        <div className="flex-[40] min-w-0 overflow-auto">
          <AnnotationSidebar
            lookup={data.lookup}
            summary={data.summary}
            onNavigateFile={onNavigateFile}
          />
        </div>
      )}
    </div>
  );
}
