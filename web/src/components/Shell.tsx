import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import type { TreeFile, FileResponse } from "../types";
import { fetchTree, fetchFile } from "../api";
import { FileTree } from "./FileTree";
import { FileViewer } from "./FileViewer";

function extractFilePath(pathname: string): string | null {
  // pathname will be like "/file/src/main.rs" from the hash router
  const prefix = "/file/";
  if (pathname.startsWith(prefix)) {
    return pathname.slice(prefix.length);
  }
  return null;
}

export function Shell() {
  const location = useLocation();
  const navigate = useNavigate();

  const [files, setFiles] = useState<TreeFile[]>([]);
  const [fileData, setFileData] = useState<FileResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedPath = extractFilePath(location.pathname);

  // Load file tree on mount
  useEffect(() => {
    fetchTree()
      .then((res) => setFiles(res.files))
      .catch((err) => setError(err.message));
  }, []);

  // Load file data when path changes
  useEffect(() => {
    if (!selectedPath) {
      setFileData(null);
      return;
    }
    setLoading(true);
    setError(null);
    fetchFile(selectedPath)
      .then((data) => {
        setFileData(data);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message);
        setFileData(null);
        setLoading(false);
      });
  }, [selectedPath]);

  const handleSelectFile = (path: string) => {
    navigate(`/file/${path}`);
  };

  const annotationCount = fileData?.annotations.length ?? 0;

  return (
    <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
      {/* Header */}
      <header className="flex shrink-0 items-center justify-between border-b border-zinc-800 px-4 py-2.5">
        <div className="flex items-center gap-3">
          <h1 className="text-base font-semibold tracking-tight text-zinc-100">
            Chronicle
          </h1>
          <span className="text-sm text-zinc-500">
            annotation viewer
          </span>
        </div>
        {selectedPath && (
          <span className="font-mono text-sm text-zinc-400">
            {selectedPath}
          </span>
        )}
      </header>

      {/* Main content */}
      <div className="flex min-h-0 flex-1">
        {/* Sidebar */}
        <aside className="w-60 shrink-0 overflow-hidden border-r border-zinc-800 bg-zinc-900/50">
          <FileTree
            files={files}
            selectedPath={selectedPath}
            onSelect={handleSelectFile}
          />
        </aside>

        {/* Content area */}
        <main className="flex-1 min-w-0 overflow-hidden">
          {error && (
            <div className="flex h-full items-center justify-center text-sm text-red-400">
              {error}
            </div>
          )}
          {loading && !error && (
            <div className="flex h-full items-center justify-center text-sm text-zinc-500">
              Loading...
            </div>
          )}
          {!loading && !error && fileData && <FileViewer data={fileData} />}
          {!loading && !error && !fileData && (
            <div className="flex h-full flex-col items-center justify-center gap-3 text-zinc-500">
              <div className="text-4xl">{ }</div>
              <p className="text-sm">Select a file to view annotations</p>
            </div>
          )}
        </main>
      </div>

      {/* Status bar */}
      <footer className="flex shrink-0 items-center border-t border-zinc-800 px-4 py-1.5 text-xs text-zinc-500">
        {selectedPath ? (
          <span>
            {annotationCount} annotation{annotationCount !== 1 ? "s" : ""} in{" "}
            <span className="font-mono text-zinc-400">{selectedPath}</span>
          </span>
        ) : (
          <span>{files.length} files indexed</span>
        )}
      </footer>
    </div>
  );
}
