import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';

export function tokenFile(root: string) {
  const hash = createHash('md5').update('https://mcp.upwork.com/mcp').digest('hex');
  return resolve(root, '.state/auth/mcp-remote-v1', `${hash}_tokens.json`);
}

export function sessionSaved(file: string): boolean {
  try {
    const tokens = JSON.parse(readFileSync(file, 'utf8'));
    return typeof tokens.access_token === 'string' && tokens.access_token.length > 0
      && (tokens.expires_at === undefined || (typeof tokens.expires_at === 'number' && tokens.expires_at > Date.now()));
  } catch { return false; }
}

export function safeErrorCode(error: unknown): string {
  let current = error;
  for (let i = 0; i < 4 && current && typeof current === 'object'; i++) {
    const value = current as { code?: unknown; cause?: unknown };
    if (typeof value.code === 'string' && /^(?:E[A-Z_]+|UND_ERR_[A-Z_]+|ERR_[A-Z_]+)$/.test(value.code)) return value.code;
    current = value.cause;
  }
  return 'REQUEST_FAILED';
}
