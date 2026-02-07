import { useMemo } from "react";
import type { FileAnnotation } from "../types";
import { RegionCard } from "./RegionCard";

const COL_GAP = 6;
const MIN_CARD_HEIGHT = 80;

interface CardLayout {
  top: number;
  minHeight: number;
  /** Which column this card occupies (0-based) */
  column: number;
  /** Total number of columns needed across all cards */
  columnCount: number;
}

/**
 * Assign each annotation to a column using greedy interval coloring.
 * Annotations are already sorted by lines.start. For each annotation,
 * pick the lowest-numbered column whose last occupant ends before this
 * annotation starts. If none are free, add a new column.
 */
function resolveLayout(
  annotations: FileAnnotation[],
  lineHeight: number,
): CardLayout[] {
  // columnEnds[c] = the bottom pixel of the last card placed in column c
  const columnEnds: number[] = [];
  const layouts: CardLayout[] = [];

  for (const ann of annotations) {
    const top = ann.lines ? (ann.lines.start - 1) * lineHeight : 0;
    const lineSpan = ann.lines
      ? (ann.lines.end - ann.lines.start + 1) * lineHeight
      : MIN_CARD_HEIGHT;
    const minHeight = Math.max(lineSpan, MIN_CARD_HEIGHT);
    const bottom = top + minHeight;

    // Find first column where this card fits without overlapping
    let col = -1;
    for (let c = 0; c < columnEnds.length; c++) {
      if (columnEnds[c] <= top) {
        col = c;
        break;
      }
    }
    if (col === -1) {
      // Need a new column
      col = columnEnds.length;
      columnEnds.push(0);
    }
    columnEnds[col] = bottom;

    layouts.push({ top, minHeight, column: col, columnCount: 0 });
  }

  // Stamp the total column count onto every layout entry
  const totalCols = Math.max(columnEnds.length, 1);
  for (const l of layouts) {
    l.columnCount = totalCols;
  }

  return layouts;
}

export function AnnotationPane({
  annotations,
  selectedIndex,
  lineHeight,
  totalHeight,
}: {
  annotations: FileAnnotation[];
  selectedIndex: number | null;
  lineHeight: number;
  totalHeight: number;
}) {
  const layouts = useMemo(
    () => resolveLayout(annotations, lineHeight),
    [annotations, lineHeight],
  );

  const lastLayout = layouts[layouts.length - 1];
  const paneHeight = lastLayout
    ? Math.max(totalHeight, lastLayout.top + lastLayout.minHeight + COL_GAP)
    : totalHeight;

  return (
    <div className="relative px-3" style={{ height: paneHeight }}>
      {annotations.map((ann, i) => {
        const { top, minHeight, column, columnCount } = layouts[i];
        // Divide available width among columns
        const colWidthPct = 100 / columnCount;
        const leftPct = column * colWidthPct;
        // Slight inset so adjacent columns have a gap
        const gapOffset = column > 0 ? COL_GAP / 2 : 0;
        const gapInset = column < columnCount - 1 ? COL_GAP / 2 : 0;

        return (
          <div
            key={`${ann.commit}-${ann.anchor.name}-${i}`}
            className="absolute"
            style={{
              top,
              minHeight,
              left: `calc(${leftPct}% + ${gapOffset}px)`,
              width: `calc(${colWidthPct}% - ${gapOffset + gapInset}px)`,
            }}
          >
            <RegionCard
              annotation={ann}
              isSelected={selectedIndex === i}
              minHeight={minHeight}
            />
          </div>
        );
      })}
    </div>
  );
}
