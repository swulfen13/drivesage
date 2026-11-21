import type { Category } from "./api";

export function fmtBytes(b: number): string {
  if (!b) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = b;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString("de-DE", { maximumFractionDigits: i === 0 ? 0 : 1 })} ${u[i]}`;
}

export function fmtNum(n: number, digits = 0): string {
  return n.toLocaleString("de-DE", { maximumFractionDigits: digits });
}

export function fmtDate(unix: number | null): string {
  if (!unix) return "–";
  return new Date(unix * 1000).toLocaleDateString("de-DE");
}

export function baseName(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

/** Path of `p` relative to `root` (for display). */
export function rel(p: string, root: string): string {
  if (root && p.startsWith(root)) return p.slice(root.length).replace(/^[\\/]/, "") || ".";
  return p;
}

export function sep(p: string): string {
  return p.includes("\\") ? "\\" : "/";
}

export const CATEGORY: Record<Category, { label: string; color: string }> = {
  documents: { label: "Dokumente", color: "#60a5fa" },
  images: { label: "Bilder", color: "#f472b6" },
  videos: { label: "Videos", color: "#a78bfa" },
  audio: { label: "Audio", color: "#fbbf24" },
  archives: { label: "Archive", color: "#fb923c" },
  installers: { label: "Installer", color: "#f87171" },
  code: { label: "Code", color: "#34d399" },
  other: { label: "Sonstiges", color: "#94a3b8" },
};

export function speedLabel(mbps: number): { label: string; tone: "good" | "ok" | "slow" } {
  if (mbps >= 1500) return { label: "Sehr schnell (NVMe)", tone: "good" };
  if (mbps >= 350) return { label: "Schnell (SSD)", tone: "good" };
  if (mbps >= 100) return { label: "Mittel (HDD)", tone: "ok" };
  return { label: "Langsam", tone: "slow" };
}
