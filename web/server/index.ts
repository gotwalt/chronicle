import express from "express";
import path from "path";
import { lsTree, showFile } from "./git.js";
import { getAnnotationsForFile, getAnnotationCounts } from "./chronicle.js";

const app = express();
const PORT = 3000;

function detectLanguage(filePath: string): string {
  const ext = path.extname(filePath).slice(1);
  const map: Record<string, string> = {
    rs: "rust",
    ts: "typescript",
    tsx: "tsx",
    js: "javascript",
    jsx: "jsx",
    py: "python",
    go: "go",
    java: "java",
    c: "c",
    cpp: "cpp",
    h: "c",
    rb: "ruby",
    md: "markdown",
    json: "json",
    toml: "toml",
    yaml: "yaml",
    yml: "yaml",
    sh: "bash",
    css: "css",
    html: "html",
  };
  return map[ext] || "text";
}

app.get("/api/tree", (_req, res) => {
  const files = lsTree();
  const counts = getAnnotationCounts();
  res.json({
    files: files.map((f) => ({ path: f, annotation_count: counts.get(f) || 0 })),
  });
});

app.use("/api/file", (req, res, next) => {
  const filePath = req.path.startsWith("/") ? req.path.slice(1) : req.path;
  if (!filePath) return next();
  try {
    const content = showFile(filePath);
    const language = detectLanguage(filePath);
    const annotations = getAnnotationsForFile(filePath);
    res.json({ path: filePath, content, language, annotations });
  } catch {
    res.status(404).json({ error: "File not found" });
  }
});

async function start() {
  if (process.env.NODE_ENV === "production") {
    app.use(express.static(path.resolve(import.meta.dirname, "../dist")));
    app.get("{*path}", (_req, res) => {
      res.sendFile(path.resolve(import.meta.dirname, "../dist/index.html"));
    });
  } else {
    const { createServer } = await import("vite");
    const vite = await createServer({
      server: { middlewareMode: true },
      appType: "spa",
    });
    app.use(vite.middlewares);
  }

  app.listen(PORT, () => {
    console.log(`Chronicle Web Viewer running at http://localhost:${PORT}`);
  });
}

start();
