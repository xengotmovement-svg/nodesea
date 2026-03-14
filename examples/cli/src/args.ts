/** Minimal argument parser — no dependencies. */

export interface ParsedArgs {
  flags: Record<string, boolean>;
  options: Record<string, string>;
  positionals: string[];
}

export function parseArgs(argv: string[]): ParsedArgs {
  const flags: Record<string, boolean> = {};
  const options: Record<string, string> = {};
  const positionals: string[] = [];

  let i = 0;
  while (i < argv.length) {
    const arg = argv[i];

    if (arg === "--") {
      positionals.push(...argv.slice(i + 1));
      break;
    }

    if (arg.startsWith("--")) {
      const name = arg.slice(2);
      const next = argv[i + 1];

      if (next !== undefined && !next.startsWith("-")) {
        options[name] = next;
        i += 2;
      } else {
        flags[name] = true;
        i += 1;
      }
    } else if (arg.startsWith("-") && arg.length === 2) {
      const name = arg.slice(1);
      const next = argv[i + 1];

      if (next !== undefined && !next.startsWith("-")) {
        options[name] = next;
        i += 2;
      } else {
        flags[name] = true;
        i += 1;
      }
    } else {
      positionals.push(arg);
      i += 1;
    }
  }

  return { flags, options, positionals };
}
