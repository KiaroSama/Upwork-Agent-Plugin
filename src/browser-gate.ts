import { EventEmitter } from 'node:events';

export default async function openAuthorization(url: string) {
  if (process.env.UPWORK_LOCAL_SIGN_IN !== '1') {
    process.stderr.write('Upwork sign-in required. Open run.ps1 and choose Sign in, then reconnect MCP.\n');
    process.exit(3);
  }
  let target: URL;
  try { target = new URL(url); } catch { process.exit(3); }
  if (target!.protocol !== 'https:' || target!.username || target!.password || /[\r\n\x1b]/.test(url)) {
    process.stderr.write('Unsafe authorization URL refused.\n');
    process.exit(3);
  }
  // The supervisor displays this only to the human, never to the persistent log.
  process.stderr.write(`AUTHORIZATION_URL ${url}\n`);
  process.stderr.write('Copy the complete link into your browser on this computer. Keep this terminal open.\n');
  const helper = new EventEmitter();
  setImmediate(() => helper.emit('exit', 0));
  return helper;
}
