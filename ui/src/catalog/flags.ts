// Country flags (SVG, from the flag-icons package): Windows does not draw flag emoji.
// Each flag is its own asset file (see vite.config.ts), fetched only when shown.

const FILES = import.meta.glob('/node_modules/flag-icons/flags/4x3/*.svg', { eager: true, query: '?url', import: 'default' }) as Record<string, string>;

const URLS: Record<string, string> = Object.fromEntries(
  Object.entries(FILES).map(([path, url]) => [path.slice(path.lastIndexOf('/') + 1, -4), url]),
);

/** URL of a flag-icons key (`gb`, `sh-ac`), or `null` if there is no such flag. */
export function flagUrl(key: string | null | undefined): string | null {
  return (key && URLS[key]) || null;
}
