// CLI tool example — TypeScript with subcommands.
//
// Demonstrates nodesea bundling a multi-file TypeScript project
// into a single standalone executable. No tsc, no tsconfig, no
// build step — rolldown handles TypeScript natively.
//
// Module graph:
//   main.ts
//   ├── src/args.ts           — argument parser
//   ├── src/colors.ts         — ANSI color helpers
//   ├── src/commands/info.ts  — system info subcommand
//   │   └── src/colors.ts       (shared)
//   ├── src/commands/hash.ts  — file hash subcommand
//   │   └── src/colors.ts       (shared)
//   └── src/commands/serve.ts — static file server subcommand
//       └── src/colors.ts       (shared)
//
// Build:  nodesea examples/cli/main.ts
// Run:    ./main info
//         ./main hash <file...> [--algo sha1]
//         ./main serve [dir] [--port 3000]

import { parseArgs } from "./src/args";
import { bold, dim, red, cyan } from "./src/colors";

const VERSION = "1.0.0";

function printHelp(): void {
  console.log(`
${bold("mycli")} ${dim(`v${VERSION}`)} — a demo CLI built with nodesea

${bold("USAGE")}
  mycli <command> [options]

${bold("COMMANDS")}
  ${cyan("info")}                     Show system information
  ${cyan("hash")} <file...>           Compute file checksums
      --algo <sha256|sha1|md5>  Hash algorithm (default: sha256)
  ${cyan("serve")} [dir]              Start a static file server
      --port <number>           Port number (default: 8080)

${bold("OPTIONS")}
  --help, -h                 Show this help message
  --version, -v              Show version
`);
}

function main(): void {
  const { flags, options, positionals } = parseArgs(process.argv.slice(2));

  if (flags["version"] || flags["v"]) {
    console.log(VERSION);
    return;
  }

  if (flags["help"] || flags["h"] || positionals.length === 0) {
    printHelp();
    return;
  }

  const command = positionals[0];
  const rest = positionals.slice(1);

  switch (command) {
    case "info": {
      const { run } = require("./src/commands/info");
      run();
      break;
    }
    case "hash": {
      const { run } = require("./src/commands/hash");
      run(rest, options);
      break;
    }
    case "serve": {
      const { run } = require("./src/commands/serve");
      run(rest, options);
      break;
    }
    default:
      console.error(red(`Unknown command: ${command}`));
      console.error(dim("Run with --help for usage."));
      process.exit(1);
  }
}

main();
