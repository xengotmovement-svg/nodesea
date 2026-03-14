import * as os from "node:os";
import * as path from "node:path";
import { bold, cyan, dim } from "../colors";

interface InfoRow {
  label: string;
  value: string;
}

function getInfo(): InfoRow[] {
  return [
    { label: "Node.js", value: process.version },
    { label: "Platform", value: `${os.platform()} ${os.arch()}` },
    { label: "CPUs", value: `${os.cpus().length}x ${os.cpus()[0].model}` },
    { label: "Memory", value: formatBytes(os.totalmem()) },
    { label: "Free memory", value: formatBytes(os.freemem()) },
    { label: "Uptime", value: formatDuration(os.uptime()) },
    { label: "Home", value: os.homedir() },
    { label: "Temp", value: os.tmpdir() },
    { label: "PID", value: `${process.pid}` },
    { label: "CWD", value: process.cwd() },
    { label: "Executable", value: process.execPath },
  ];
}

function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let val = bytes;
  while (val >= 1024 && i < units.length - 1) {
    val /= 1024;
    i++;
  }
  return `${val.toFixed(1)} ${units[i]}`;
}

function formatDuration(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const parts: string[] = [];
  if (d > 0) parts.push(`${d}d`);
  if (h > 0) parts.push(`${h}h`);
  parts.push(`${m}m`);
  return parts.join(" ");
}

export function run(): void {
  const rows = getInfo();
  const maxLabel = Math.max(...rows.map((r) => r.label.length));

  console.log(bold("\nSystem Information\n"));
  for (const row of rows) {
    const label = cyan(row.label.padStart(maxLabel));
    console.log(`  ${label}  ${row.value}`);
  }
  console.log();
}
