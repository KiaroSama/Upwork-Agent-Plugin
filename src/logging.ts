import { mkdirSync, openSync, writeSync, closeSync } from 'node:fs';
import { resolve } from 'node:path';

export function createLog(root: string, component: string) {
  let fd: number | undefined;
  const date = new Date().toISOString().slice(0, 19).replace('T', '_').replace(/:/g, '-');
  try {
    mkdirSync(resolve(root, 'logs'), { recursive: true });
    const name = `${component}_${date}_UTC`;
    for (let i = 0; i < 100; i++) {
      try { fd = openSync(resolve(root, 'logs', `${name}${i ? `_${i}` : ''}.log`), 'wx', 0o600); break; }
      catch (e) { if ((e as NodeJS.ErrnoException).code !== 'EEXIST') throw e; }
    }
    if (fd === undefined) throw new Error('Log filename collision limit exceeded.');
  } catch { process.stderr.write('File logging unavailable; using safe stderr diagnostics.\n'); }
  function log(level: string, message: string) {
    const timestamp = new Date().toISOString().slice(0, 19).replace('T', ' ');
    const line = `[${timestamp} UTC] [${level}] [${component}] ${message}\n`;
    try { if (fd !== undefined) writeSync(fd, line, undefined, 'utf8'); else process.stderr.write(line); }
    catch { process.stderr.write('Diagnostic log write failed.\n'); }
  }
  const close = () => { if (fd !== undefined) { closeSync(fd); fd = undefined; } };
  return { log, close };
}
