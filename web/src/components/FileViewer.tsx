import { useRef, useState } from "react";
import type { FileResponse } from "../types";
import { SourcePane } from "./SourcePane";
import { AnnotationPane } from "./AnnotationPane";

const LINE_HEIGHT = 24; // matches leading-6 (1.5rem at 16px base)

export function FileViewer({ data }: { data: FileResponse }) {
  const [selectedRegion, setSelectedRegion] = useState<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const lineCount = data.content.split("\n").length;
  const totalHeight = lineCount * LINE_HEIGHT;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Annotation count header */}
      <div className="shrink-0 border-b border-zinc-800 px-4 py-2">
        <span className="text-xs font-medium text-zinc-400">
          {data.annotations.length > 0
            ? `${data.annotations.length} annotated region${data.annotations.length > 1 ? "s" : ""}`
            : "No annotations"}
        </span>
      </div>
      {/* Shared scroll container for both panes */}
      <div ref={scrollRef} className="flex-1 overflow-auto min-h-0">
        <div className="flex" style={{ minHeight: totalHeight }}>
          {/* Source code */}
          <div className="flex-[55] min-w-0 border-r border-zinc-800">
            <SourcePane
              content={data.content}
              language={data.language}
              annotations={data.annotations}
              onRegionClick={setSelectedRegion}
            />
          </div>
          {/* Annotations — positioned to align with source lines */}
          <div className="flex-[35] min-w-0 relative">
            <AnnotationPane
              annotations={data.annotations}
              selectedIndex={selectedRegion}
              lineHeight={LINE_HEIGHT}
              totalHeight={totalHeight}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
