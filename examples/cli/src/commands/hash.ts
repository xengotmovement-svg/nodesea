import * as fs from "node:fs";
import * as crypto from "node:crypto";
import * as path from "node:path";
import { bold, green, red, dim } from "../colors";

type Algorithm = "sha256" | "sha1" | "md5";

function hashFile(filePath: string, algo: Algorithm): string {
  const data = fs.readFileSync(filePath);
  return crypto.createHash(algo).update(data).digest("hex");
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function run(
  files: string[],
  options: Record<string, string>
): void {
  const algo = (options["algo"] || options["a"] || "sha256") as Algorithm;

  if (files.length === 0) {
    console.error(red("error:") + " no files specified");
    console.error(dim("usage: mycli hash <file...> [--algo sha256|sha1|md5]"));
    process.exit(1);
  }

  console.log(bold(`\n${algo.toUpperCase()} checksums\n`));

  for (const file of files) {
    const resolved = path.resolve(file);

    if (!fs.existsSync(resolved)) {
      console.log(`  ${red("missing")}  ${file}`);
      continue;
    }

    const stat = fs.statSync(resolved);
    if (stat.isDirectory()) {
      console.log(`  ${red("is dir")}   ${file}`);
      continue;
    }

    const hash = hashFile(resolved, algo);
    const size = dim(`(${formatSize(stat.size)})`);
    console.log(`  ${green(hash)}  ${file} ${size}`);
  }

  console.log();
}
