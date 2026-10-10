import { watchFile, unwatchFile } from 'node:fs';
import { Agent, ProxyAgent, Socks5ProxyAgent, setGlobalDispatcher } from 'undici';
import type { Dispatcher } from 'undici';
import { readConnections, validateConnections } from './connections.ts';
import type { Connections } from './connections.ts';

export function createRoute(config: Connections): Dispatcher {
  validateConnections(config);
  const options = { connectTimeout: 5000, headersTimeout: 30000, bodyTimeout: 30000, connections: 2 };
  if (config.mode === 'blocked') throw new Error('Connection blocked. Select a proxy in run.ps1 first.');
  if (config.mode === 'direct') return new Agent(options);
  const proxy = new URL(config.profiles.find(p => p.name === config.selected)!.url);
  if (proxy.protocol === 'socks5h:') proxy.protocol = 'socks5:';
  return proxy.protocol === 'socks5:'
    ? new Socks5ProxyAgent(proxy, options)
    : new ProxyAgent({ ...options, uri: proxy.href, proxyTunnel: true });
}

export function installRoute(file: string, onChanged: () => never): Dispatcher {
  const config = readConnections(file);
  const snapshot = JSON.stringify(config);
  const route = createRoute(config);
  const dispatch = route.dispatch.bind(route);
  route.dispatch = (options, handler) => {
    let current: string;
    try { current = JSON.stringify(readConnections(file)); }
    catch { onChanged(); }
    if (current! !== snapshot) onChanged();
    return dispatch(options, handler);
  };
  watchFile(file, { interval: 100 }, () => {
    try { if (JSON.stringify(readConnections(file)) === snapshot) return; } catch { /* Fail closed. */ }
    onChanged();
  });
  const destroy = route.destroy.bind(route);
  route.destroy = (...args) => { unwatchFile(file); return destroy(...args); };
  setGlobalDispatcher(route);
  return route;
}
