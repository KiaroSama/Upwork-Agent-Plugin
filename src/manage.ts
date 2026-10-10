import { readFileSync, writeFileSync, renameSync, mkdirSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { readConnections, updateConnections, safeProfile } from './connections.ts';
import { createLog } from './logging.ts';
import { importProxyFile, ProxyImportError } from './proxy-import.ts';

const root = fileURLToPath(new URL('../', import.meta.url));
const file = resolve(root, 'config/connections.json');
const { log, close } = createLog(root, 'upwork-menu');
const [command, name] = process.argv.slice(2);
try {
  if (command === 'status') {
    const c = readConnections(file);
    console.log(JSON.stringify({ mode: c.mode, selected: c.selected, profiles: c.profiles.map(safeProfile) }, null, 2));
  } else if (command === 'import') {
    if (!name) throw new ProxyImportError('Enter a proxy file path.');
    const result = importProxyFile(file, name);
    console.log(`Imported ${result.imported} proxy profiles; skipped ${result.skipped} duplicates. Active route unchanged.`);
  } else if (command === 'add') {
    let input = '';
    for await (const chunk of process.stdin) {
      input += chunk;
      if (input.length > 8192) throw new Error('Proxy input is too large.');
    }
    let profile;
    try { profile = JSON.parse(input.replace(/^﻿/, '')); } catch { throw new Error('Proxy input must be valid JSON.'); }
    updateConnections(file, c => c.profiles.push(profile));
    console.log('Proxy saved.');
  } else if (['select', 'direct', 'block'].includes(command)) {
    updateConnections(file, c => {
      c.mode = command === 'select' ? 'proxy' : command === 'direct' ? 'direct' : 'blocked';
      c.selected = command === 'select' ? name : null;
    });
    console.log('Route saved. Active connector stops; reconnect MCP to apply.');
  } else if (command === 'register') {
    const settingsFile = resolve(root, '.claude/settings.local.json');
    const mcpFile = resolve(root, '.mcp.json');
    const read = (p: string) => existsSync(p) ? JSON.parse(readFileSync(p, 'utf8')) : {};
    const settings = read(settingsFile);
    const mcp = read(mcpFile);
    const deny = { serverUrl: 'https://mcp.upwork.com/mcp' };
    settings.deniedMcpServers ??= [];
    if (!Array.isArray(settings.deniedMcpServers)) throw new Error('Invalid existing MCP deny configuration.');
    if (!settings.deniedMcpServers.some((x: { serverUrl?: string }) => x.serverUrl === deny.serverUrl)) settings.deniedMcpServers.push(deny);
    mcp.mcpServers ??= {};
    if (mcp.mcpServers['upwork-local'] && mcp.mcpServers['upwork-local'].args?.[0] !== resolve(root, 'src/bridge.ts')) {
      throw new Error('An unrelated upwork-local entry already exists; registration refused.');
    }
    mcp.mcpServers['upwork-local'] = {
      type: 'stdio', command: process.execPath, args: [resolve(root, 'src/bridge.ts')],
      env: { NODE_OPTIONS: '', NODE_DEBUG: '', NODE_USE_ENV_PROXY: '0', NODE_TLS_REJECT_UNAUTHORIZED: '1', UPWORK_LOCAL_SIGN_IN: '0',
        NODE_COMPILE_CACHE: resolve(root, '.cache/compile'), TEMP: resolve(root, '.cache/tmp'), TMP: resolve(root, '.cache/tmp') },
    };
    for (const [path, value] of [[settingsFile, settings], [mcpFile, mcp]] as const) {
      mkdirSync(dirname(path), { recursive: true });
      const temp = `${path}.${randomUUID()}.tmp`;
      writeFileSync(temp, JSON.stringify(value, null, 2) + '\n', { encoding: 'utf8', flag: 'wx', mode: 0o600 });
      renameSync(temp, path);
    }
    console.log('Project upwork-local MCP registered. Original direct Upwork entries denied; skills preserved. Reopen this project session and approve MCP if prompted.');
  } else throw new Error('Usage: manage.ts status|import PATH|add|select NAME|direct|block|register');
  log('INFO', `Operation completed command=${['status','import','add','select','direct','block','register'].includes(command) ? command : 'invalid'}`);
} catch (error) {
  log('ERROR', 'Operation failed; private input suppressed.');
  console.error(error instanceof ProxyImportError ? error.message
    : 'Operation failed. Check profile name, URL, configuration permissions, or an existing lock. No direct fallback enabled.');
  process.exitCode = 1;
} finally { close(); }
