import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync, writeFileSync, readdirSync } from 'node:fs';

const root = fileURLToPath(new URL('../', import.meta.url));
const temp = resolve(root, '.ci-work');
mkdirSync(temp, { recursive: true });
const args = ['--test', '--test-timeout=10000', '--test-concurrency=1'];
if (process.argv[2]) args.push(`--test-name-pattern=${process.argv[2]}`);
const suites = readdirSync(resolve(root, 'tests'), { recursive: true })
  .filter(name => typeof name === 'string' && name.endsWith('.test.ts'))
  .map(name => resolve(root, 'tests', String(name))).sort();
if (!suites.length) throw new Error('No test suites discovered.');
args.push(...suites);
const child = spawn(process.execPath, args, {
  cwd: root, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, TEMP: temp, TMP: temp, TMPDIR: temp, NODE_COMPILE_CACHE: resolve(temp, 'compile'),
    NODE_EXTRA_CA_CERTS: resolve(root, 'tests/fixtures/localhost-test.crt') },
});
const started = Date.now();
let lastOutput = started;
let expired = false;
const stop = () => {
  if (expired) return;
  expired = true;
  if (process.platform === 'win32') {
    spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
  } else child.kill('SIGKILL');
};
for (const [stream, output] of [[child.stdout, process.stdout], [child.stderr, process.stderr]] as const) {
  stream.on('data', data => { lastOutput = Date.now(); output.write(data); });
}
const timer = setInterval(() => {
  if (Date.now() - started > 60000 || Date.now() - lastOutput > 20000) stop();
}, 200);
child.on('error', () => { clearInterval(timer); process.exitCode = 1; });
child.on('close', code => {
  clearInterval(timer);
  writeFileSync(resolve(temp, 'verification.json'), JSON.stringify({
    pid: child.pid, started, elapsedMs: Date.now() - started, exitCode: code, expired,
  }) + '\n', 'utf8');
  if (expired) console.error('Verification exceeded its wall/idle bound; owned process tree terminated.');
  process.exitCode = expired ? 1 : (code ?? 1);
});
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
