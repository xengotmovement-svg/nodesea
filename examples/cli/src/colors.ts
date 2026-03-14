/** ANSI color helpers. */

const enabled =
  process.stdout.isTTY && !process.env["NO_COLOR"];

function wrap(code: number, text: string): string {
  return enabled ? `\x1b[${code}m${text}\x1b[0m` : text;
}

export const bold    = (s: string) => wrap(1, s);
export const dim     = (s: string) => wrap(2, s);
export const red     = (s: string) => wrap(31, s);
export const green   = (s: string) => wrap(32, s);
export const yellow  = (s: string) => wrap(33, s);
export const blue    = (s: string) => wrap(34, s);
export const cyan    = (s: string) => wrap(36, s);
