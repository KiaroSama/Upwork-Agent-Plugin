import { readFileSync, mkdirSync, openSync, closeSync, writeFileSync, renameSync, unlinkSync } from 'node:fs';
import { dirname } from 'node:path';
import { randomUUID } from 'node:crypto';

export type Connections = {
  version: 1;
  mode: 'blocked' | 'direct' | 'proxy';
  selected: string | null;
  profiles: { name: string; url: string; label?: string }[];
};

export function validateConnections(value: unknown): Connections {
  const c = value as Connections;
  if (!c || c.version !== 1 || !['blocked', 'direct', 'proxy'].includes(c.mode)
    || !(c.selected === null || typeof c.selected === 'string') || !Array.isArray(c.profiles)) {
    throw new Error('Invalid connection configuration; networking is blocked.');
  }
  const names = new Set<string>();
  for (const p of c.profiles) {
    if (!p || typeof p.name !== 'string' || !/^[a-zA-Z0-9_-]{1,64}$/.test(p.name) || names.has(p.name)) {
      throw new Error('Proxy names must be unique identifiers (1–64 letters, digits, _ or -).');
    }
    if (p.label !== undefined && (typeof p.label !== 'string' || !p.label.trim()
      || p.label.length > 128 || /[\x00-\x1f\x7f]/.test(p.label))) {
      throw new Error('Proxy display names must contain 1–128 printable characters.');
    }
    names.add(p.name);
    let u: URL;
    try { u = new URL(p.url); } catch { throw new Error('Invalid proxy URL.'); }
    if (!['http:', 'https:', 'socks5:', 'socks5h:'].includes(u.protocol) || !u.hostname
      || u.search || u.hash || (u.pathname && u.pathname !== '/')
      || !/:\d+\/?$/.test(p.url) || Number(u.port || (u.protocol === 'https:' ? 443 : 80)) < 1
      || Number(u.port || (u.protocol === 'https:' ? 443 : 80)) > 65535) {
      throw new Error('Proxy URL requires HTTP, HTTPS or SOCKS5, a host and port, without path/query/fragment.');
    }
    try { decodeURIComponent(u.username); decodeURIComponent(u.password); }
    catch { throw new Error('Invalid proxy credential encoding.'); }
  }
  if (c.mode === 'proxy' && (!c.selected || !names.has(c.selected))) {
    throw new Error('Selected proxy does not exist; networking is blocked.');
  }
  return c;
}

export function readConnections(file: string): Connections {
  try { return validateConnections(JSON.parse(readFileSync(file, 'utf8'))); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return { version: 1, mode: 'blocked', selected: null, profiles: [] };
    }
    throw error;
  }
}

export function updateConnections(file: string, change: (c: Connections) => void): Connections {
  mkdirSync(dirname(file), { recursive: true });
  const lock = `${file}.lock`;
  let fd: number;
  try { fd = openSync(lock, 'wx', 0o600); }
  catch { throw new Error('Configuration is busy; retry or inspect its stale lock.'); }
  const temp = `${file}.${randomUUID()}.tmp`;
  try {
    const c = readConnections(file);
    change(c);
    validateConnections(c);
    writeFileSync(temp, JSON.stringify(c, null, 2) + '\n', { encoding: 'utf8', mode: 0o600, flag: 'wx' });
    renameSync(temp, file);
    return c;
  } finally {
    closeSync(fd);
    try { unlinkSync(temp); } catch (e) { if ((e as NodeJS.ErrnoException).code !== 'ENOENT') throw e; }
    unlinkSync(lock);
  }
}

export function safeProfile(p: Connections['profiles'][number]) {
  const u = new URL(p.url);
  u.username = ''; u.password = '';
  return { name: p.name, label: p.label ?? p.name, url: u.href };
}
