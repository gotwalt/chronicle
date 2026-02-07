import type { TreeResponse, FileResponse } from "./types";

const BASE = "/api";

export async function fetchTree(): Promise<TreeResponse> {
  const res = await fetch(`${BASE}/tree`);
  if (!res.ok) throw new Error(`Failed to fetch tree: ${res.statusText}`);
  return res.json();
}

export async function fetchFile(path: string): Promise<FileResponse> {
  const res = await fetch(`${BASE}/file/${path}`);
  if (!res.ok) throw new Error(`Failed to fetch file: ${res.statusText}`);
  return res.json();
}
