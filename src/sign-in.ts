import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createLog } from './logging.ts';
import { sessionSaved, tokenFile } from './auth-state.ts';

export async function completeSignIn(root: string, options: {
  args?: string[]; tokenPath?: string; timeoutMs?: number; display?: (text: string) => void;
} = {}): Promise<number> {
  const display = options.display ?? (text => process.stdout.write(text));
  const { log, close } = createLog(root, 'upwork-sign-in');
  const child = spawn(process.execPath, options.args ?? [resolve(root, 'src/bridge.ts'), '--sign-in'], {
    cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, NODE_OPTIONS: '', NODE_DEBUG: '', UPWORK_LOCAL_SIGN_IN: '1' },
  });
  let protocol = '', diagnostics = '';
  let finished = false;
  let outcome = 1;
  let timer: ReturnType<typeof setTimeout>;
  let cancel: () => void = () => {};
  const onCancel = () => cancel();
  process.once('SIGINT', onCancel); process.once('SIGTERM', onCancel);
  const result = new Promise<number>(resolveResult => {
    const finish = (code: number, message: string) => {
      if (finished) return;
      finished = true; outcome = code;
      clearTimeout(timer);
      display(message + '\n'); log(code ? 'ERROR' : 'INFO', message);
      if (child.exitCode === null) child.kill();
    };
    cancel = () => finish(1, 'Sign-in cancelled. No completion claimed.');
    child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
    child.stdout.on('data', chunk => {
      protocol += chunk;
      if (protocol.length > 1024 * 1024) { finish(1, 'Login failed: invalid protocol output.'); return; }
      let newline: number;
      while ((newline = protocol.indexOf('\n')) >= 0) {
        const line = protocol.slice(0, newline); protocol = protocol.slice(newline + 1);
        try {
          const response = JSON.parse(line);
          if (response.id !== 1) continue;
          if (response.error || !response.result?.serverInfo || !sessionSaved(options.tokenPath ?? tokenFile(root))) {
            finish(1, 'Login failed: no usable session saved. Check the browser callback and try again.');
          } else finish(0, 'Sign-in complete. Session saved locally. Reconnect upwork-local in Claude.');
        } catch { finish(1, 'Login failed: invalid protocol response.'); }
      }
    });
    child.stderr.on('data', chunk => {
      diagnostics += chunk;
      if (diagnostics.length > 65536) { finish(1, 'Login failed: excessive diagnostic output.'); return; }
      let newline: number;
      while ((newline = diagnostics.indexOf('\n')) >= 0) {
        const line = diagnostics.slice(0, newline).trim(); diagnostics = diagnostics.slice(newline + 1);
        if (line.startsWith('AUTHORIZATION_URL ')) {
          const url = line.slice('AUTHORIZATION_URL '.length);
          try {
            const target = new URL(url);
            if (target.protocol !== 'https:' || target.username || target.password || /[\r\n\x1b]/.test(url)) throw new Error();
            display(`\nCopy this complete link into your external browser on this computer:\n${url}\n\nKeep this terminal open until the session is saved.\n`);
            log('INFO', 'Authorization link shown to human; waiting for callback.');
          } catch { finish(1, 'Login failed: unsafe authorization link.'); }
        } else if (/^LOGIN_EVENT [A-Z0-9_]+$/.test(line)) {
          display(line.replace('LOGIN_EVENT ', 'Login: ') + '\n'); log('INFO', line);
        }
      }
    });
    child.on('error', () => { finish(1, 'Login helper could not start. Check Node and project permissions.'); });
    child.on('close', code => {
      if (!finished) finish(1, `Login did not complete (exit ${code ?? 'unknown'}). Check the newest sign-in log.`);
      clearTimeout(timer); process.removeListener('SIGINT', onCancel); process.removeListener('SIGTERM', onCancel);
      close(); resolveResult(outcome);
    });
    timer = setTimeout(() => finish(1, 'Login timed out. Run Sign in again for a fresh link.'), options.timeoutMs ?? 300000);
    child.stdin.on('error', () => {});
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {
      protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'upwork-local-sign-in', version: '1.0.0' },
    } }) + '\n');
    log('INFO', 'Sign-in helper started; waiting for protocol and stored session.');
  });
  return result;
}

const root = fileURLToPath(new URL('../', import.meta.url));
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  process.exitCode = await completeSignIn(root);
}
