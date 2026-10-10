import { readFileSync, mkdirSync, existsSync } from 'node:fs';
import { tokenFile, safeErrorCode } from './auth-state.ts';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { registerHooks } from 'node:module';
import { installRoute } from './routing.ts';
import { readConnections } from './connections.ts';
import { createLog } from './logging.ts';

const root = fileURLToPath(new URL('../', import.meta.url));
const file = resolve(root, 'config/connections.json');
const signIn = process.argv.length === 3 && process.argv[2] === '--sign-in';
const { log, close } = createLog(root, 'upwork-bridge');
process.on('exit', code => { log(code ? 'ERROR' : 'INFO', `Stopped exit_code=${code}`); close(); });
process.on('uncaughtException', () => { log('ERROR', 'Unhandled runtime error; details suppressed to protect credentials.'); process.exit(1); });
process.on('unhandledRejection', () => { log('ERROR', 'Unhandled runtime rejection; connection closed.'); process.exit(1); });

try {
  if (process.argv.length > 2 && !signIn) throw new Error('Unsupported bridge option.');
  const config = readConnections(file);
  installRoute(file, () => {
    log('WARNING', 'Configuration changed or became unreadable; closing old route.');
    process.stderr.write('Route changed. Reconnect upwork-local MCP to use the new selection.\n');
    process.exit(4);
  });
  for (const key of ['NODE_OPTIONS', 'NODE_DEBUG', 'DEBUG', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY',
    'NO_PROXY', 'http_proxy', 'https_proxy', 'all_proxy', 'no_proxy', 'NODE_USE_ENV_PROXY', 'NODE_TLS_REJECT_UNAUTHORIZED']) delete process.env[key];
  const state = resolve(root, '.state/auth');
  const temp = resolve(root, '.cache/tmp');
  mkdirSync(state, { recursive: true }); mkdirSync(temp, { recursive: true });
  process.env.MCP_REMOTE_CONFIG_DIR = state;
  process.env.TEMP = temp; process.env.TMP = temp; process.env.TMPDIR = temp;
  process.env.UPWORK_LOCAL_SIGN_IN = signIn ? '1' : '0';
  const tokens = tokenFile(root);
  if (!signIn && !existsSync(tokens)) {
    process.stderr.write('Upwork sign-in required. Open run.ps1 and choose Sign in, then reconnect MCP.\n');
    process.exit(3);
  }
  const upstream = JSON.parse(readFileSync(resolve(root, 'node_modules/mcp-remote/package.json'), 'utf8'));
  if (upstream.version !== '0.14.3') throw new Error('Unverified bridge dependency version. Reinstall from the lockfile.');
  registerHooks({ resolve(specifier, context, nextResolve) {
    if (specifier === 'open') return { url: new URL('./browser-gate.ts', import.meta.url).href, shortCircuit: true };
    return nextResolve(specifier, context);
  } });
  // Never retain upstream payloads or URLs; map only fixed diagnostic categories.
  const report = (event: string) => {
    log(event.startsWith('FAILED') ? 'ERROR' : 'INFO', event);
    if (signIn) process.stderr.write(`LOGIN_EVENT ${event}\n`);
  };
  console.log = console.info = console.warn = console.debug = () => {};
  console.error = (message: unknown, ...details: unknown[]) => {
    if (typeof message !== 'string') return;
    if (message.includes('Discovering OAuth')) report('DISCOVERING_AUTH');
    else if (message.includes('Connected to remote server')) report('REMOTE_CONNECTED');
    else if (message.includes('Local STDIO server running')) report('WAITING_FOR_INITIALIZE');
    else if (message.includes('Fatal error:')) report(`FAILED_${safeErrorCode(details[0])}`);
  };
  log('INFO', `Started node=${process.version} route=${config.mode} manual_sign_in=${signIn}`);
  process.argv = [process.execPath, resolve(root, 'node_modules/mcp-remote/dist/proxy.js'),
    'https://mcp.upwork.com/mcp', '--transport', 'http-only', '--auth-timeout', '300'];
  await import('../node_modules/mcp-remote/dist/proxy.js');
} catch (error) {
  log('ERROR', 'Startup failed; networking remains blocked.');
  const message = error instanceof Error && !/https?:|socks|token|password/i.test(error.message)
    ? error.message : 'Connector startup failed. Check configuration and dependencies.';
  process.stderr.write(message + '\n');
  process.exit(1);
}
