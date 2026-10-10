import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, readFileSync, writeFileSync, readdirSync, cpSync, mkdirSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { setGlobalDispatcher, fetch as routedFetch } from 'undici';
import { createRoute, installRoute } from '../src/routing.ts';
import { fixtures } from './proxy-fixtures.ts';

async function command(args: string[], input = '', executable = process.execPath, cwd = root) {
  const child = spawn(executable, args, { cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stdout.on('data', b => stdout += b);
  child.stderr.on('data', b => stderr += b);
  child.stdin.end(input);
  const timer = setTimeout(() => child.kill(), executable === process.execPath ? 4000 : 8000);
  try { const [code] = await once(child, 'close'); return { code, stdout, stderr }; }
  finally { clearTimeout(timer); if (child.exitCode === null) child.kill(); }
}


const root = fileURLToPath(new URL('../', import.meta.url));

test('profiles persist and invalid selections never enable direct mode', { timeout: 5000 }, async () => {
  const { readConnections, updateConnections } = await import('../src/connections.ts');
  const dir = mkdtempSync(resolve(root, '.ci-work/config-'));
  const file = resolve(dir, 'connections.json');
  try {
    assert.equal(readConnections(file).mode, 'blocked');
    updateConnections(file, c => { c.profiles.push({ name: 'local', url: 'socks5://127.0.0.1:1080' }); });
    updateConnections(file, c => { c.mode = 'proxy'; c.selected = 'local'; });
    assert.equal(readConnections(file).selected, 'local');
    assert.throws(() => updateConnections(file, c => { c.selected = 'missing'; }));
    assert.equal(readConnections(file).mode, 'proxy');
    assert.throws(() => updateConnections(file, c => { c.profiles.push({ name: 'bad', url: 'ftp://localhost:21' }); }));
    assert.equal(readConnections(file).profiles.length, 1);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('file import preserves labels and route, deduplicates and rolls back malformed input', { timeout: 5000 }, async () => {
  const { importProxyFile } = await import('../src/proxy-import.ts');
  const { readConnections, updateConnections, safeProfile } = await import('../src/connections.ts');
  const dir = mkdtempSync(resolve(root, '.ci-work/import-'));
  const file = resolve(dir, 'config.json');
  const list = resolve(dir, 'Proxy list.txt');
  try {
    updateConnections(file, c => { c.mode = 'direct'; });
    writeFileSync(list, '﻿socks5://proxy.example:12004:example-user:p@ss%word:UK 5\r\nhttp://proxy.example:8080:user:pass:UK 1\r\n', 'utf8');
    assert.deepEqual(importProxyFile(file, `"${list}"`), { imported: 2, skipped: 0 });
    const c = readConnections(file);
    assert.equal(c.mode, 'direct');
    assert.equal(c.profiles[0].label, 'UK 5');
    assert.equal(new URL(c.profiles[0].url).username, 'example-user');
    assert.equal(decodeURIComponent(new URL(c.profiles[0].url).password), 'p@ss%word');
    assert.equal(safeProfile(c.profiles[0]).label, 'UK 5');
    assert.doesNotMatch(JSON.stringify(safeProfile(c.profiles[0])), /example-user|p%40ss/);
    assert.deepEqual(importProxyFile(file, list), { imported: 0, skipped: 2 });
    const before = readFileSync(file, 'utf8');
    writeFileSync(list, 'socks5://other.example:1080:user:pass:UK 2\ninvalid private data\n', 'utf8');
    assert.throws(() => importProxyFile(file, list), /Invalid proxy record at line 2/);
    assert.equal(readFileSync(file, 'utf8'), before);
    writeFileSync(list, Buffer.from([0xff]));
    assert.throws(() => importProxyFile(file, list), /UTF-8/);
    assert.equal(readFileSync(file, 'utf8'), before);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('launcher renders vertical menu and safely accepts quoted file paths and back', { timeout: 10000 }, async () => {
  const dir = mkdtempSync(resolve(root, '.ci-work/menu '));
  cpSync(resolve(root, 'src'), resolve(dir, 'src'), { recursive: true });
  cpSync(resolve(root, 'run.ps1'), resolve(dir, 'run.ps1'));
  writeFileSync(resolve(dir, 'package.json'), '{"type":"module"}\n', 'utf8');
  mkdirSync(resolve(dir, 'node_modules/mcp-remote'), { recursive: true });
  writeFileSync(resolve(dir, 'node_modules/mcp-remote/package.json'), '{"name":"test-only-presence-marker"}', 'utf8');
  try {
  const preview = await command(['-NoProfile', '-File', 'run.ps1', '-Preview'], '', 'pwsh', dir);
  assert.equal(preview.code, 0);
  assert.match(preview.stdout, /Upwork Main menu:/);
  assert.match(preview.stdout, /1\. Select proxy \[1\]/);
  assert.match(preview.stdout, /2\. Import proxy list from file/);
  assert.doesNotMatch(preview.stdout, /"profiles"|"mode"/);
  const menu = await command(['-NoProfile', '-File', 'run.ps1'], '2\n"missing file.txt"\n2\n0\nexit\n', 'pwsh', dir);
  assert.equal(menu.code, 0);
  assert.match(menu.stderr + menu.stdout, /Cannot read proxy file/);
  assert.match(menu.stdout, /back=0, quit=exit/);
  assert.equal((menu.stdout.match(/Upwork Main menu:/g) ?? []).length, 3);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('explicit proxy routes native and upstream fetch, with remote DNS and no fallback', { timeout: 10000 }, async () => {
  const f = await fixtures();
  const require = createRequire(new URL('../node_modules/mcp-remote/dist/proxy.js', import.meta.url));
  const legacyFetch = require('undici').fetch;
  try {
    for (const [scheme, port] of [['http', f.proxyPort], ['socks5', f.socksPort]] as const) {
      const route = createRoute({ version: 1, mode: 'proxy', selected: 'test',
        profiles: [{ name: 'test', url: `${scheme}://127.0.0.1:${port}` }] });
      setGlobalDispatcher(route);
      try {
        for (const fetcher of [fetch, legacyFetch, routedFetch]) {
          const response = await fetcher(`http://unresolvable.invalid:${f.targetPort}/redirect`, { signal: AbortSignal.timeout(2000) });
          assert.equal(await response.text(), 'routed');
        }
      } finally { await route.destroy(); }
    }
    for (const [scheme, port] of [['http', f.proxyPort], ['https', f.secureProxyPort], ['socks5', f.socksPort]] as const) {
      const route = createRoute({ version: 1, mode: 'proxy', selected: 'tls',
        profiles: [{ name: 'tls', url: `${scheme}://127.0.0.1:${port}` }] });
      setGlobalDispatcher(route);
      try {
        const response = await fetch(`https://localhost:${f.securePort}/`, { signal: AbortSignal.timeout(2000) });
        assert.equal(await response.text(), 'secure');
      } finally { await route.destroy(); }
    }
    assert.ok(f.domains.includes('unresolvable.invalid'));
    const direct = createRoute({ version: 1, mode: 'direct', selected: null, profiles: [] });
    setGlobalDispatcher(direct);
    try { assert.equal(await (await fetch(`http://127.0.0.1:${f.targetPort}/`, { signal: AbortSignal.timeout(2000) })).text(), 'routed'); }
    finally { await direct.destroy(); }
    f.refuse();
    const before = f.count();
    for (const [scheme, port] of [['http', f.proxyPort], ['socks5', f.socksPort]] as const) {
      const route = createRoute({ version: 1, mode: 'proxy', selected: 'test',
        profiles: [{ name: 'test', url: `${scheme}://127.0.0.1:${port}` }] });
      setGlobalDispatcher(route);
      try {
        await assert.rejects(fetch(`http://127.0.0.1:${f.targetPort}/`, { signal: AbortSignal.timeout(2000) }));
        assert.equal(f.count(), before);
      } finally { await route.destroy(); }
    }
  } finally { await f.close(); }
});

test('upstream stdio bridge speaks MCP through the selected local proxy', { timeout: 7000 }, async () => {
  const f = await fixtures();
  const dir = mkdtempSync(resolve(root, '.ci-work/protocol-'));
  const script = `import { ProxyAgent,setGlobalDispatcher } from 'undici';
    setGlobalDispatcher(new ProxyAgent({uri:'http://127.0.0.1:${f.proxyPort}',proxyTunnel:true}));
    process.argv=[process.execPath,'proxy','http://unresolvable.invalid:${f.targetPort}/mcp','--allow-http','--transport','http-only','--silent'];
    await import('./node_modules/mcp-remote/dist/proxy.js');`;
  const child = spawn(process.execPath, ['--input-type=module', '-e', script], {
    cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, MCP_REMOTE_CONFIG_DIR: dir },
  });
  let output = '', errors = '';
  let readyResolve: () => void;
  const ready = new Promise<void>(r => readyResolve = r);
  child.stdout.on('data', b => { output += b; if (output.includes('local-fixture')) readyResolve(); });
  child.stderr.on('data', b => errors += b);
  const timer = setTimeout(() => { child.kill(); readyResolve(); }, 4000);
  try {
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {
      protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'test', version: '1.0.0' },
    } }) + '\n');
    await ready;
    assert.match(output, /local-fixture/);
    assert.equal(JSON.parse(output.trim().split('\n')[0]).id, 1);
    assert.ok(f.domains.includes('unresolvable.invalid'));
  } finally {
    clearTimeout(timer);
    if (child.exitCode === null) { const ended = once(child, 'close'); child.kill(); await ended; }
    await f.close(); rmSync(dir, { recursive: true, force: true });
  }
});

test('route change closes the old process without reconnecting automatically', { timeout: 5000 }, async () => {
  const dir = mkdtempSync(resolve(root, '.ci-work/switch-'));
  const file = resolve(dir, 'connections.json');
  const { updateConnections } = await import('../src/connections.ts');
  updateConnections(file, c => { c.mode = 'direct'; });
  const script = `import { installRoute } from ${JSON.stringify(new URL('../src/routing.ts', import.meta.url).href)};
    installRoute(${JSON.stringify(file)}, () => process.exit(4)); console.log('ready');`;
  const child = spawn(process.execPath, ['--input-type=module', '-e', script], { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  const timer = setTimeout(() => child.kill(), 3000);
  try {
    await once(child.stdout, 'data');
    const closed = once(child, 'close');
    updateConnections(file, c => { c.mode = 'blocked'; });
    const [code] = await closed;
    assert.equal(code, 4);
  } finally { clearTimeout(timer); if (child.exitCode === null) child.kill(); rmSync(dir, { recursive: true, force: true }); }
});

test('manual login gate prints complete URL without launching a browser', { timeout: 5000 }, async () => {
  const result = await command(['--input-type=module', '-e', `
    process.env.UPWORK_LOCAL_SIGN_IN='1';
    const {default:open}=await import('./src/browser-gate.ts');
    const child=await open('https://example.com/authorize?state=example&code_challenge=abc%2B123');
    await new Promise(resolve=>child.once('exit',resolve));
  `]);
  assert.equal(result.code, 0);
  assert.match(result.stderr, /https:\/\/example.com\/authorize\?state=example&code_challenge=abc%2B123/);
  assert.match(result.stderr, /Copy/);
});

test('sign-in supervisor requires stored tokens and never logs authorization link', { timeout: 5000 }, async () => {
  const { completeSignIn } = await import('../src/sign-in.ts');
  const dir = mkdtempSync(resolve(root, '.ci-work/login-'));
  const tokenPath = resolve(dir, 'tokens.json');
  let shown = '';
  const script = `import{writeFileSync}from'node:fs';process.stdin.once('data',()=>{
    process.stderr.write('AUTHORIZATION_URL https://example.com/authorize?state=fixture-private\\n');
    writeFileSync(${JSON.stringify(tokenPath)},JSON.stringify({access_token:'test-only-token',expires_at:Date.now()+60000}),'utf8');
    console.log(JSON.stringify({jsonrpc:'2.0',id:1,result:{serverInfo:{name:'fixture',version:'1.0.0'}}}));
  });process.stdin.resume();`;
  try {
    assert.equal(await completeSignIn(dir, { args: ['--input-type=module','-e',script], tokenPath, timeoutMs: 2000,
      display: text => shown += text }), 0);
    assert.match(shown, /state=fixture-private/);
    assert.match(shown, /Session saved locally/);
    for (const file of readdirSync(resolve(dir,'logs'))) {
      assert.doesNotMatch(readFileSync(resolve(dir,'logs',file),'utf8'), /fixture-private|test-only-token|https:/);
    }
    rmSync(tokenPath);
    const incomplete = `process.stdin.once('data',()=>{console.log(JSON.stringify({jsonrpc:'2.0',id:1,result:{serverInfo:{name:'fixture'}}}))});process.stdin.resume();`;
    assert.equal(await completeSignIn(dir, { args:['--input-type=module','-e',incomplete], tokenPath, timeoutMs:2000, display:()=>{} }), 1);
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('project registration preserves skills, blocked entry and logs stay credential-free', { timeout: 10000 }, async () => {
  const dir = mkdtempSync(resolve(root, '.ci-work/clean checkout '));
  cpSync(resolve(root, 'src'), resolve(dir, 'src'), { recursive: true });
  writeFileSync(resolve(dir, 'package.json'), '{"type":"module"}\n', 'utf8');
  const run = (args: string[]) => command(args, '', process.execPath, dir);
  try {
    const result = await run(['src/manage.ts', 'status']);
    assert.equal(result.code, 0);
    assert.deepEqual(JSON.parse(result.stdout), { mode: 'blocked', selected: null, profiles: [] });
    const blocked = await run(['src/bridge.ts']);
    assert.equal(blocked.code, 1);
    assert.equal(blocked.stdout, '');
    assert.match(blocked.stderr, /Connection blocked/);
    assert.equal((await run(['src/manage.ts', 'direct'])).code, 0);
    const unsigned = await run(['src/bridge.ts']);
    assert.equal(unsigned.code, 3);
    assert.equal(unsigned.stdout, '');
    assert.match(unsigned.stderr, /sign-in required/);
    mkdirSync(resolve(dir, '.claude'));
    writeFileSync(resolve(dir, '.claude/settings.local.json'), JSON.stringify({ enabledPlugins: { 'example-plugin': true } }), 'utf8');
    writeFileSync(resolve(dir, '.mcp.json'), JSON.stringify({ mcpServers: { other: { type: 'stdio', command: 'example' } } }), 'utf8');
    assert.equal((await run(['src/manage.ts', 'register'])).code, 0);
    assert.equal((await run(['src/manage.ts', 'register'])).code, 0);
    const mcp = JSON.parse(readFileSync(resolve(dir, '.mcp.json'), 'utf8'));
    const settings = JSON.parse(readFileSync(resolve(dir, '.claude/settings.local.json'), 'utf8'));
    assert.equal(mcp.mcpServers['upwork-local'].type, 'stdio');
    assert.equal(mcp.mcpServers['upwork-local'].args[0], resolve(dir, 'src/bridge.ts'));
    assert.equal(mcp.mcpServers.other.command, 'example');
    assert.deepEqual(settings.deniedMcpServers, [{ serverUrl: 'https://mcp.upwork.com/mcp' }]);
    assert.deepEqual(settings.enabledPlugins, { 'example-plugin': true });
    const logs = readdirSync(resolve(dir, 'logs')).filter(f => f.endsWith('.log'));
    assert.ok(logs.some(f => /^upwork-bridge_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}_UTC/.test(f)));
    for (const file of logs) {
      const text = readFileSync(resolve(dir, 'logs', file), 'utf8');
      assert.doesNotMatch(text, /Authorization:|access_token|refresh_token|password|socks5:\/\//);
      assert.match(text, /\[\d{4}-\d{2}-\d{2} .* UTC\] \[(INFO|ERROR|WARNING)\]/);
    }
    const gate = await run(['--input-type=module', '-e',
      `import { registerHooks } from 'node:module'; registerHooks({resolve(s,c,n){return s==='open'?{url:new URL('./src/browser-gate.ts',import.meta.url).href,shortCircuit:true}:n(s,c)}}); const {default:open}=await import('open'); await open('https://example.com/authorize?code=private-test');`]);
    assert.equal(gate.code, 3);
    assert.doesNotMatch(gate.stderr, /private-test/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
