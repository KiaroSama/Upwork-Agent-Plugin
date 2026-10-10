import http from 'node:http';
import https from 'node:https';
import net from 'node:net';
import { readFileSync } from 'node:fs';
import { once } from 'node:events';

export const tlsOptions = {
  key: readFileSync(new URL('./fixtures/localhost-test.key', import.meta.url)),
  cert: readFileSync(new URL('./fixtures/localhost-test.crt', import.meta.url)),
};

export async function listen(server: net.Server) {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return (server.address() as net.AddressInfo).port;
}

export async function fixtures() {
  const sockets = new Set<net.Socket>();
  let requests = 0;
  const domains: string[] = [];
  const target = http.createServer((req, res) => {
    requests++;
    if (req.url === '/mcp') {
      if (req.method !== 'POST') { res.writeHead(405).end(); return; }
      let body = '';
      req.setEncoding('utf8'); req.on('data', chunk => body += chunk);
      req.on('end', () => {
        const message = JSON.parse(body);
        if (!('id' in message)) { res.writeHead(202).end(); return; }
        const result = message.method === 'initialize'
          ? { protocolVersion: '2025-11-25', capabilities: { tools: {} }, serverInfo: { name: 'local-fixture', version: '1.0.0' } }
          : message.method === 'tools/list' ? { tools: [] } : {};
        res.setHeader('Content-Type', 'application/json');
        res.end(JSON.stringify({ jsonrpc: '2.0', id: message.id, result }));
      });
    } else if (req.url === '/redirect') { res.writeHead(302, { location: '/done' }); res.end(); }
    else { res.end('routed'); }
  });
  const secure = https.createServer(tlsOptions, (_req, res) => { requests++; res.end('secure'); });
  const targetPort = await listen(target);
  const securePort = await listen(secure);
  let refused = false;
  function connect(req: http.IncomingMessage, socket: net.Socket, head: Buffer) {
    if (refused) { socket.end('HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n'); return; }
    const u = new URL(`http://${req.url}`);
    domains.push(u.hostname);
    const upstream = net.connect({ host: '127.0.0.1', port: Number(u.port) });
    sockets.add(upstream);
    upstream.on('error', () => socket.destroy());
    upstream.once('connect', () => {
      socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
      if (head.length) upstream.write(head);
      socket.pipe(upstream); upstream.pipe(socket);
    });
    socket.on('close', () => upstream.destroy());
  }
  const proxy = http.createServer((_req, res) => res.writeHead(500).end());
  const secureProxy = https.createServer(tlsOptions, (_req, res) => res.writeHead(500).end());
  proxy.on('connect', connect); secureProxy.on('connect', connect);
  const proxyPort = await listen(proxy);
  const secureProxyPort = await listen(secureProxy);
  const socks = net.createServer(socket => {
    let stage = 0;
    let buffer = Buffer.alloc(0);
    const receive = (data: Buffer) => {
      buffer = Buffer.concat([buffer, data]);
      if (stage === 0) {
        if (buffer.length < 2 || buffer.length < 2 + buffer[1]) return;
        buffer = buffer.subarray(2 + buffer[1]);
        socket.write(Buffer.from([5, 0])); stage = 1;
      }
      if (stage === 1) {
        if (buffer.length < 5) return;
        if (buffer[3] !== 3) { socket.destroy(); return; }
        const length = buffer[4];
        if (buffer.length < 7 + length) return;
        domains.push(buffer.subarray(5, 5 + length).toString('utf8'));
        const port = buffer.readUInt16BE(5 + length);
        if (refused) { socket.end(Buffer.from([5, 5, 0, 1, 0, 0, 0, 0, 0, 0])); return; }
        const pending = buffer.subarray(7 + length);
        socket.removeListener('data', receive);
        const upstream = net.connect({ host: '127.0.0.1', port });
        sockets.add(upstream);
        upstream.on('error', () => socket.destroy());
        upstream.once('connect', () => {
          socket.write(Buffer.from([5, 0, 0, 1, 127, 0, 0, 1, 0, 0]));
          if (pending.length) upstream.write(pending);
          socket.pipe(upstream); upstream.pipe(socket);
        });
        socket.on('close', () => upstream.destroy());
        stage = 2;
      }
    };
    socket.on('data', receive);
  });
  const socksPort = await listen(socks);
  const servers = [target, secure, proxy, secureProxy, socks];
  for (const server of servers) server.on('connection', socket => {
    sockets.add(socket); socket.on('error', () => {}); socket.on('close', () => sockets.delete(socket));
  });
  return {
    targetPort, securePort, proxyPort, secureProxyPort, socksPort, domains,
    count: () => requests,
    refuse: () => { refused = true; for (const socket of sockets) socket.destroy(); },
    close: async () => {
      for (const socket of sockets) socket.destroy();
      await Promise.all(servers.map(server => new Promise<void>((resolve, reject) => server.close(e => e ? reject(e) : resolve()))));
    },
  };
}
