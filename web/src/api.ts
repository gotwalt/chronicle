import type {
  TreeResponse,
  FileViewResponse,
  StatusOutput,
  DecisionsOutput,
  KnowledgeStore,
} from "./types";

const BASE = "/api";

export async function fetchTree(): Promise<TreeResponse> {
  const res = await fetch(`${BASE}/tree`);
  if (!res.ok) throw new Error(`Failed to fetch tree: ${res.statusText}`);
  return res.json();
}

export async function fetchFileView(path: string): Promise<FileViewResponse> {
  const res = await fetch(`${BASE}/file-view/${path}`);
  if (!res.ok) throw new Error(`Failed to fetch file: ${res.statusText}`);
  return res.json();
}

export async function fetchStatus(): Promise<StatusOutput> {
  const res = await fetch(`${BASE}/status`);
  if (!res.ok) throw new Error(`Failed to fetch status: ${res.statusText}`);
  return res.json();
}

export async function fetchDecisions(path?: string): Promise<DecisionsOutput> {
  const params = path ? `?path=${encodeURIComponent(path)}` : "";
  const res = await fetch(`${BASE}/decisions${params}`);
  if (!res.ok)
    throw new Error(`Failed to fetch decisions: ${res.statusText}`);
  return res.json();
}

export async function fetchKnowledge(): Promise<KnowledgeStore> {
  const res = await fetch(`${BASE}/knowledge`);
  if (!res.ok)
    throw new Error(`Failed to fetch knowledge: ${res.statusText}`);
  return res.json();
}
