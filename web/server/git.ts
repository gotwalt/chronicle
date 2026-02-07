import { execSync } from "child_process";
import path from "path";

const REPO_ROOT = path.resolve(import.meta.dirname, "../..");

function git(args: string, suppressStderr = false): string {
  return execSync(`git ${args}`, {
    cwd: REPO_ROOT,
    encoding: "utf-8",
    stdio: suppressStderr ? ["pipe", "pipe", "ignore"] : undefined,
  });
}

export function lsTree(commit = "HEAD"): string[] {
  try {
    return git(`ls-tree -r --name-only ${commit}`)
      .split("\n")
      .filter(Boolean);
  } catch {
    return [];
  }
}

export function showFile(filePath: string, commit = "HEAD"): string {
  return git(`show ${commit}:${filePath}`);
}

export function logForFile(filePath: string): string[] {
  try {
    return git(`log --follow --format=%H -- ${filePath}`)
      .split("\n")
      .filter(Boolean);
  } catch {
    return [];
  }
}

export function noteRead(commit: string): string | null {
  try {
    return git(`notes --ref refs/notes/chronicle show ${commit}`, true);
  } catch {
    return null;
  }
}

export function listAnnotatedCommits(): string[] {
  try {
    return git("notes --ref refs/notes/chronicle list")
      .split("\n")
      .filter(Boolean)
      .map((line) => line.split(" ")[1])
      .filter((sha): sha is string => Boolean(sha));
  } catch {
    return [];
  }
}
