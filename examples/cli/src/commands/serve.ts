import * as http from "node:http";
import * as fs from "node:fs";
import * as path from "node:path";
import { bold, cyan, dim, green, yellow } from "../colors";

const MIME_TYPES: Record<string, string> = {
  ".html": "text/html",
  ".css": "text/css",
  ".js": "application/javascript",
  ".json": "application/json",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".svg": "image/svg+xml",
  ".txt": "text/plain",
};

function getMime(ext: string): string {
  return MIME_TYPES[ext] || "application/octet-stream";
}

export function run(
  positionals: string[],
  options: Record<string, string>
): void {
  const root = path.resolve(positionals[0] || ".");
  const port = parseInt(options["port"] || options["p"] || "8080", 10);

  if (!fs.existsSync(root) || !fs.statSync(root).isDirectory()) {
    console.error(`Not a directory: ${root}`);
    process.exit(1);
  }

  const server = http.createServer((req, res) => {
    const url = new URL(req.url || "/", `http://localhost:${port}`);
    let filePath = path.join(root, url.pathname);

    if (fs.existsSync(filePath) && fs.statSync(filePath).isDirectory()) {
      filePath = path.join(filePath, "index.html");
    }

    if (!fs.existsSync(filePath)) {
      res.writeHead(404, { "Content-Type": "text/plain" });
      res.end("404 Not Found\n");
      console.log(`  ${yellow("404")} ${req.method} ${url.pathname}`);
      return;
    }

    const ext = path.extname(filePath);
    const mime = getMime(ext);
    const data = fs.readFileSync(filePath);

    res.writeHead(200, { "Content-Type": mime });
    res.end(data);
    console.log(`  ${green("200")} ${req.method} ${url.pathname} ${dim(mime)}`);
  });

  server.listen(port, () => {
    console.log(bold("\nStatic file server\n"));
    console.log(`  ${cyan("Root")}  ${root}`);
    console.log(`  ${cyan("URL")}   http://localhost:${port}/`);
    console.log(dim("\n  Press Ctrl+C to stop.\n"));
  });
}
