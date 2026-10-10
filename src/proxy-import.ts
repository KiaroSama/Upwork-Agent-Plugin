import { readFileSync, statSync } from 'node:fs';
import { updateConnections, validateConnections } from './connections.ts';

export class ProxyImportError extends Error {}

export function importProxyFile(configFile: string, inputPath: string) {
  const path = inputPath.trim().replace(/^(["'])(.*)\1$/, '$2');
  let text: string;
  try {
    if (statSync(path).size > 1024 * 1024) throw new ProxyImportError('Proxy file exceeds 1 MiB.');
    text = new TextDecoder('utf-8', { fatal: true }).decode(readFileSync(path));
  } catch (error) {
    if (error instanceof ProxyImportError) throw error;
    throw new ProxyImportError('Cannot read proxy file. Check the path, permissions and UTF-8 encoding.');
  }
  const records: { url: string; label: string }[] = [];
  for (const [index, raw] of text.split(/\r?\n/).entries()) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    try {
      const match = line.match(/^(https?|socks5h?):\/\/([^:\s]+|\[[^\]]+\]):(\d+):([^:]*):([^:]*):(.+)$/i);
      let url: string, label: string;
      if (match) {
        const [, scheme, host, port, username, password, display] = match;
        const u = new URL(`${scheme.toLowerCase()}://${host}:${port}`);
        u.username = encodeURIComponent(username); u.password = encodeURIComponent(password);
        // Keep explicit default ports for existing configuration validation.
        url = `${u.protocol}//${u.username || u.password ? `${u.username}:${u.password}@` : ''}${u.hostname}:${port}`;
        label = display.trim();
      } else {
        const u = new URL(line);
        label = `Proxy ${index + 1}`;
        url = line;
      }
      validateConnections({ version: 1, mode: 'blocked', selected: null,
        profiles: [{ name: 'import-check', label, url }] });
      records.push({ label, url });
    } catch { throw new ProxyImportError(`Invalid proxy record at line ${index + 1}. No profiles imported.`); }
  }
  if (!records.length) throw new ProxyImportError('Proxy file contains no records.');
  let imported = 0, skipped = 0;
  updateConnections(configFile, config => {
    const urls = new Set(config.profiles.map(p => new URL(p.url).href));
    const names = new Set(config.profiles.map(p => p.name));
    for (const record of records) {
      const normalized = new URL(record.url).href;
      if (urls.has(normalized)) { skipped++; continue; }
      const base = record.label.replace(/[^a-zA-Z0-9_-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 54) || 'proxy';
      let name = base, suffix = 2;
      while (names.has(name)) name = `${base}-${suffix++}`;
      config.profiles.push({ name, label: record.label, url: record.url });
      names.add(name); urls.add(normalized); imported++;
    }
  });
  return { imported, skipped };
}
