import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import type { TreeFile, FileViewResponse, StatusOutput } from "../types";
import { fetchTree, fetchFileView, fetchStatus } from "../api";
import { FileTree } from "./FileTree";
import { FileViewer } from "./FileViewer";
import { OverviewPage } from "./OverviewPage";
import { DecisionsPage } from "./DecisionsPage";
import { KnowledgePage } from "./KnowledgePage";

function extractFilePath(pathname: string): string | null {
  const prefix = "/file/";
  if (pathname.startsWith(prefix)) {
    return pathname.slice(prefix.length);
  }
  return null;
}

type Page = "overview" | "file" | "decisions" | "knowledge";

function getPage(pathname: string): Page {
  if (pathname.startsWith("/file/")) return "file";
  if (pathname === "/decisions") return "decisions";
  if (pathname === "/knowledge") return "knowledge";
  return "overview";
}

export function Shell() {
  const location = useLocation();
  const navigate = useNavigate();

  const [files, setFiles] = useState<TreeFile[]>([]);
  const [fileData, setFileData] = useState<FileViewResponse | null>(null);
  const [status, setStatus] = useState<StatusOutput | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const page = getPage(location.pathname);
  const selectedPath = extractFilePath(location.pathname);

  // Load file tree and status on mount
  useEffect(() => {
    fetchTree()
      .then((res) => setFiles(res.files))
      .catch((err) => setError(err.message));
    fetchStatus()
      .then((s) => setStatus(s))
      .catch(() => {}); // non-critical
  }, []);

  // Load file data when path changes
  useEffect(() => {
    if (!selectedPath) {
      setFileData(null);
      return;
    }
    setLoading(true);
    setError(null);
    fetchFileView(selectedPath)
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

  return (
    <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
      {/* Header */}
      <header className="flex shrink-0 items-center justify-between border-b border-zinc-800 px-4 py-2.5">
        <div className="flex items-center gap-4">
          <button
            onClick={() => navigate("/")}
            className="text-base font-semibold tracking-tight text-zinc-100 hover:text-emerald-400 transition-colors cursor-pointer"
          >
            Chronicle
          </button>
          <nav className="flex items-center gap-1">
            <NavButton
              label="Files"
              active={page === "overview" || page === "file"}
              onClick={() => navigate("/")}
            />
            <NavButton
              label="Decisions"
              active={page === "decisions"}
              onClick={() => navigate("/decisions")}
            />
            <NavButton
              label="Knowledge"
              active={page === "knowledge"}
              onClick={() => navigate("/knowledge")}
            />
          </nav>
        </div>
        {selectedPath && (
          <span className="font-mono text-sm text-zinc-400">
            {selectedPath}
          </span>
        )}
      </header>

      {/* Main content */}
      <div className="flex min-h-0 flex-1">
        {/* Sidebar — show for overview and file pages */}
        {(page === "overview" || page === "file") && (
          <aside className="w-60 shrink-0 overflow-hidden border-r border-zinc-800 bg-zinc-900/50">
            <FileTree
              files={files}
              selectedPath={selectedPath}
              onSelect={handleSelectFile}
            />
          </aside>
        )}

        {/* Content area */}
        <main className="flex-1 min-w-0 overflow-hidden">
          {error && (
            <div className="flex h-full items-center justify-center text-sm text-red-400">
              {error}
            </div>
          )}

          {page === "overview" && !error && (
            <OverviewPage status={status} files={files} onSelectFile={handleSelectFile} />
          )}

          {page === "file" && loading && !error && (
            <div className="flex h-full items-center justify-center text-sm text-zinc-500">
              Loading...
            </div>
          )}
          {page === "file" && !loading && !error && fileData && (
            <FileViewer data={fileData} onNavigateFile={handleSelectFile} />
          )}
          {page === "file" && !loading && !error && !fileData && (
            <div className="flex h-full flex-col items-center justify-center gap-3 text-zinc-500">
              <p className="text-sm">Select a file to view annotations</p>
            </div>
          )}

          {page === "decisions" && !error && <DecisionsPage />}
          {page === "knowledge" && !error && <KnowledgePage />}
        </main>
      </div>

      {/* Status bar */}
      <footer className="flex shrink-0 items-center border-t border-zinc-800 px-4 py-1.5 text-xs text-zinc-500">
        {status ? (
          <span>
            {status.total_annotations} annotations | {status.coverage_pct}%
            coverage ({status.recent_annotated}/{status.recent_commits} recent)
          </span>
        ) : (
          <span>{files.length} files indexed</span>
        )}
      </footer>
    </div>
  );
}

function NavButton({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`cursor-pointer rounded px-2.5 py-1 text-sm transition-colors ${
        active
          ? "bg-zinc-800 text-zinc-100"
          : "text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50"
      }`}
    >
      {label}
    </button>
  );
}
