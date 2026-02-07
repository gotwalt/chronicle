import { useState, useMemo, useEffect } from "react";
import type { TreeFile } from "../types";

interface TreeNode {
  name: string;
  fullPath: string;
  children: Map<string, TreeNode>;
  file: TreeFile | null;
}

function buildTree(files: TreeFile[]): TreeNode {
  const root: TreeNode = {
    name: "",
    fullPath: "",
    children: new Map(),
    file: null,
  };
  for (const file of files) {
    const parts = file.path.split("/");
    let node = root;
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      if (!node.children.has(part)) {
        node.children.set(part, {
          name: part,
          fullPath: parts.slice(0, i + 1).join("/"),
          children: new Map(),
          file: null,
        });
      }
      node = node.children.get(part)!;
    }
    node.file = file;
  }
  return root;
}

function getAncestorPaths(path: string): Set<string> {
  const parts = path.split("/");
  const result = new Set<string>();
  for (let i = 1; i <= parts.length; i++) {
    result.add(parts.slice(0, i).join("/"));
  }
  return result;
}

function FolderNode({
  node,
  selectedPath,
  onSelect,
  expanded,
  onToggle,
  depth,
}: {
  node: TreeNode;
  selectedPath: string | null;
  onSelect: (path: string) => void;
  expanded: Set<string>;
  onToggle: (path: string) => void;
  depth: number;
}) {
  const sortedChildren = useMemo(() => {
    const entries = Array.from(node.children.values());
    // Folders first, then files, alphabetically within each group
    return entries.sort((a, b) => {
      const aIsFolder = a.children.size > 0 && !a.file;
      const bIsFolder = b.children.size > 0 && !b.file;
      if (aIsFolder && !bIsFolder) return -1;
      if (!aIsFolder && bIsFolder) return 1;
      return a.name.localeCompare(b.name);
    });
  }, [node.children]);

  return (
    <div>
      {sortedChildren.map((child) => {
        const isFolder = child.children.size > 0;
        const isFile = child.file !== null;
        const isExpanded = expanded.has(child.fullPath);
        const isSelected = selectedPath === child.fullPath;

        if (isFolder && !isFile) {
          return (
            <div key={child.fullPath}>
              <button
                onClick={() => onToggle(child.fullPath)}
                className={`flex w-full items-center gap-1.5 py-1 pr-2 text-left text-sm hover:bg-zinc-800/50 transition-colors cursor-pointer ${
                  isSelected ? "text-emerald-400" : "text-zinc-400"
                }`}
                style={{ paddingLeft: `${depth * 12 + 8}px` }}
              >
                <span
                  className={`text-[10px] transition-transform ${isExpanded ? "rotate-90" : ""}`}
                >
                  ▸
                </span>
                <span>{child.name}</span>
              </button>
              {isExpanded && (
                <FolderNode
                  node={child}
                  selectedPath={selectedPath}
                  onSelect={onSelect}
                  expanded={expanded}
                  onToggle={onToggle}
                  depth={depth + 1}
                />
              )}
            </div>
          );
        }

        // File leaf (may also have children if it's both a directory-like path component and a file)
        return (
          <div key={child.fullPath}>
            <button
              onClick={() => onSelect(child.fullPath)}
              className={`flex w-full items-center gap-1.5 py-1 pr-2 text-left text-sm transition-colors cursor-pointer ${
                isSelected
                  ? "bg-zinc-800 text-emerald-400"
                  : "text-zinc-300 hover:bg-zinc-800/50 hover:text-zinc-200"
              }`}
              style={{ paddingLeft: `${depth * 12 + 20}px` }}
            >
              <span
                className={`inline-block h-1.5 w-1.5 shrink-0 rounded-full ${
                  child.file && child.file.annotation_count > 0
                    ? "bg-emerald-500"
                    : "bg-zinc-600"
                }`}
              />
              <span className="truncate">{child.name}</span>
            </button>
            {isFolder && isExpanded && (
              <FolderNode
                node={child}
                selectedPath={selectedPath}
                onSelect={onSelect}
                expanded={expanded}
                onToggle={onToggle}
                depth={depth + 1}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

export function FileTree({
  files,
  selectedPath,
  onSelect,
}: {
  files: TreeFile[];
  selectedPath: string | null;
  onSelect: (path: string) => void;
}) {
  const tree = useMemo(() => buildTree(files), [files]);

  const [expanded, setExpanded] = useState<Set<string>>(() => {
    // Expand root-level folders by default
    const initial = new Set<string>();
    for (const child of tree.children.values()) {
      if (child.children.size > 0) {
        initial.add(child.fullPath);
      }
    }
    return initial;
  });

  // When selectedPath changes, expand ancestors
  useEffect(() => {
    if (selectedPath) {
      const ancestors = getAncestorPaths(selectedPath);
      setExpanded((prev) => {
        const next = new Set(prev);
        let changed = false;
        for (const a of ancestors) {
          if (!next.has(a)) {
            next.add(a);
            changed = true;
          }
        }
        return changed ? next : prev;
      });
    }
  }, [selectedPath]);

  const onToggle = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  return (
    <div className="h-full overflow-y-auto py-2">
      <FolderNode
        node={tree}
        selectedPath={selectedPath}
        onSelect={onSelect}
        expanded={expanded}
        onToggle={onToggle}
        depth={0}
      />
    </div>
  );
}
