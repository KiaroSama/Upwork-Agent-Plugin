"""Bounded native tests for overlapping refresh and manual authorization.

All services and credentials are synthetic. TLS is terminated locally and never
forwarded. Only child environments receive disposable certificate trust.
"""
from __future__ import annotations
import argparse
import http.client
import json
import logging
import os
from pathlib import Path
import queue
import socket
import subprocess
import tempfile
import threading
import time
import urllib.parse
import protocol_probe as protocol

LOG = logging.getLogger('credential-coordination')

class Handler(protocol.Handler):
    def do_POST(self):
        if urllib.parse.urlsplit(self.path).path != '/token':
            return super().do_POST()
        length = int(self.headers.get('Content-Length', '0'))
        if not 0 < length <= 8192:
            self.reply(None, 400)
            return
        fields = urllib.parse.parse_qs(self.rfile.read(length).decode('utf-8'))
        if fields.get('grant_type') == ['refresh_token']:
            with self.server.counter_lock:
                self.server.refresh_count += 1
                index = self.server.refresh_count
            if index == 1:
                self.server.refresh_started.set()
                if not self.server.release_refresh.wait(12):
                    self.reply(None, 503)
                    return
            elif fields.get('refresh_token') != ['synthetic-refresh-1']:
                self.server.rotated_token_ok = False
            self.reply({'access_token': f'synthetic-refreshed-{index}', 'token_type': 'Bearer', 'expires_in': 3600, 'refresh_token': f'synthetic-refresh-{index}'})
        elif fields.get('grant_type') == ['authorization_code']:
            self.server.manual_exchange.set()
            self.reply({'access_token': 'synthetic-manual-session', 'token_type': 'Bearer', 'expires_in': 3600, 'refresh_token': 'synthetic-manual-refresh'})
        else:
            self.reply(None, 400)

class Fixture(protocol.Fixture):
    def __init__(self, root: Path):
        super().__init__(root, 'valid')
        self.RequestHandlerClass = Handler
        self.refresh_started = threading.Event()
        self.release_refresh = threading.Event()
        self.manual_exchange = threading.Event()
        self.counter_lock = threading.Lock()
        self.refresh_count = 0
        self.rotated_token_ok = True

class Child:
    def __init__(self, binary: Path, root: Path, env: dict, command: str):
        self.lines = queue.Queue()
        self.records = []
        self.process = subprocess.Popen([str(binary), '--root', str(root), command], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8', env=env)
        def read(stream, forward):
            for line in stream:
                if forward:
                    self.records.append(line)
                    self.lines.put(line)
        self.readers = [threading.Thread(target=read, args=(self.process.stdout, True), daemon=True),
                        threading.Thread(target=read, args=(self.process.stderr, False), daemon=True)]
        for reader in self.readers:
            reader.start()
    def initialize(self):
        request = {'jsonrpc':'2.0', 'id':41, 'method':'initialize', 'params':{
            'protocolVersion':'2025-11-25', 'capabilities':{}, 'clientInfo':{'name':'coordination-fixture','version':'1'}}}
        self.process.stdin.write(json.dumps(request) + '\n')
        self.process.stdin.flush()
    def matching_line(self, predicate, budget=8):
        deadline = time.monotonic() + budget
        while time.monotonic() < deadline:
            try:
                line = self.lines.get(timeout=.05)
            except queue.Empty:
                if self.process.poll() is not None:
                    return None
                continue
            if predicate(line):
                return line
        return None
    def initialized(self):
        def match(line):
            try:
                value = json.loads(line)
                return value.get('id') == 41 and 'result' in value
            except (ValueError, AttributeError):
                return False
        return self.matching_line(match) is not None
    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=3)
        for reader in self.readers:
            reader.join(timeout=1)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            stream.close()


def run_case(binary: Path, certificates: Path, work: Path, mode: str) -> dict:
    with tempfile.TemporaryDirectory(prefix='credential-race-', dir=work) as tmp:
        root = Path(tmp)
        (root / 'config').mkdir()
        (root / '.state/rust').mkdir(parents=True)
        with socket.socket() as reserved:
            reserved.bind(('127.0.0.1', 0))
            callback_port = reserved.getsockname()[1]
        server = Fixture(certificates)
        thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .05}, daemon=True)
        thread.start()
        (root/'config/connections.json').write_text(json.dumps({'version':1,'mode':'proxy','selected':'fixture','profiles':[{'name':'fixture','url':f'http://127.0.0.1:{server.server_port}'}]}), encoding='utf-8')
        client = {'client_id':'fixture-client','redirect_uris':[f'http://127.0.0.1:{callback_port}/callback']}
        credentials = {'client_id':'fixture-client','token_response':{'access_token':'synthetic-old-session','token_type':'Bearer','expires_in':1,'refresh_token':'synthetic-refresh-0'},'token_received_at':1,'granted_scopes':[],'issuer':protocol.ISSUER}
        for name, value in [('client.json', client), ('credentials.json', credentials)]:
            path = root/'.state/rust'/name
            path.write_text(json.dumps(value), encoding='utf-8')
            path.chmod(0o600)
        env = os.environ.copy()
        empty = certificates/'empty'
        empty.mkdir(exist_ok=True)
        env.update({'SSL_CERT_FILE':str(certificates/'ca.pem'), 'SSL_CERT_DIR':str(empty), 'TEMP':str(root), 'TMP':str(root), 'TMPDIR':str(root)})
        children = []
        callback_thread = None
        callback_ok = threading.Event()
        receipt = {'case':mode}
        try:
            first = Child(binary, root, env, 'bridge')
            children.append(first)
            first.initialize()
            if not server.refresh_started.wait(8):
                raise RuntimeError('Initial refresh did not reach the synthetic barrier.')
            if mode == 'manual_during_refresh':
                second = Child(binary, root, env, 'sign-in')
                children.append(second)
                line = second.matching_line(lambda text: text.startswith('https://'))
                if line is None:
                    raise RuntimeError('Synthetic sign-in did not produce an authorization link.')
                params = urllib.parse.parse_qs(urllib.parse.urlsplit(line.strip()).query)
                redirect = urllib.parse.urlsplit(params['redirect_uri'][0])
                if redirect.hostname != '127.0.0.1' or redirect.port != callback_port:
                    raise RuntimeError('Unexpected synthetic callback destination.')
                def callback():
                    connection = http.client.HTTPConnection('127.0.0.1', callback_port, timeout=8)
                    try:
                        query = urllib.parse.urlencode({'code':'synthetic-code','state':params['state'][0],'iss':protocol.ISSUER})
                        connection.request('GET', redirect.path+'?'+query)
                        response = connection.getresponse()
                        response.read()
                        if response.status == 200:
                            callback_ok.set()
                    except (OSError, http.client.HTTPException):
                        pass
                    finally:
                        connection.close()
                callback_thread = threading.Thread(target=callback, daemon=True)
                callback_thread.start()
                receipt['manual_exchanged_before_refresh_release'] = server.manual_exchange.wait(.5)
                if receipt['manual_exchanged_before_refresh_release']:
                    callback_thread.join(timeout=9)
                    second.process.wait(timeout=8)
                server.release_refresh.set()
                callback_thread.join(timeout=9)
                second.process.wait(timeout=8)
                receipt['callback_ok'] = callback_ok.is_set()
                receipt['manual_exit_success'] = second.process.returncode == 0
                receipt['completion_claimed'] = any('Sign-in complete.' in text for text in second.records)
                receipt['first_bridge_initialized'] = first.initialized()
                saved = json.loads((root/'.state/rust/credentials.json').read_text(encoding='utf-8'))
                receipt['manual_session_preserved'] = saved['token_response']['access_token'] == 'synthetic-manual-session'
            else:
                second = Child(binary, root, env, 'bridge')
                children.append(second)
                second.initialize()
                # Observe the blocked interval; only an owned native process is polled.
                deadline = time.monotonic() + .5
                while time.monotonic() < deadline and second.process.poll() is None:
                    time.sleep(.01)
                receipt['second_exited_while_first_refresh_active'] = second.process.poll() is not None
                server.release_refresh.set()
                receipt['first_bridge_initialized'] = first.initialized()
                receipt['second_bridge_initialized'] = second.initialized()
                receipt['rotated_token_reloaded'] = server.rotated_token_ok
            receipt['refresh_requests'] = server.refresh_count
            LOG.info('Completed synthetic coordination case %s.', mode)
            return receipt
        finally:
            server.release_refresh.set()
            for child in reversed(children):
                child.close()
            if callback_thread is not None:
                callback_thread.join(timeout=1)
            server.shutdown()
            server.server_close()
            thread.join(timeout=1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-baseline', action='store_true')
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] [credential-coordination] %(message)s')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    LOG.debug('Using local gates, disposable credentials, and child-only TLS trust.')
    try:
        with tempfile.TemporaryDirectory(prefix='credential-certs-', dir=args.output.parent) as tmp:
            certs = Path(tmp)
            protocol.certificates(certs)
            results = [run_case(args.binary.resolve(), certs, args.output.parent, name)
                       for name in ['manual_during_refresh','concurrent_refresh']]
        args.output.write_text(json.dumps(results, indent=2)+'\n', encoding='utf-8')
        manual, refresh = results
        common = manual['callback_ok'] and manual['manual_exit_success'] and manual['completion_claimed'] and manual['first_bridge_initialized'] and refresh['first_bridge_initialized']
        if args.expect_baseline:
            passed = (common and manual['manual_exchanged_before_refresh_release'] and not manual['manual_session_preserved']
                      and not refresh['second_exited_while_first_refresh_active'] and refresh['second_bridge_initialized']
                      and refresh['rotated_token_reloaded'] and refresh['refresh_requests'] == 2)
        else:
            passed = (common and not manual['manual_exchanged_before_refresh_release'] and manual['manual_session_preserved']
                      and not refresh['second_exited_while_first_refresh_active'] and refresh['second_bridge_initialized']
                      and refresh['rotated_token_reloaded'] and refresh['refresh_requests'] == 2)
        LOG.warning('Synthetic coordination evidence does not exercise a live Upwork account.')
        if not passed:
            LOG.error('Coordination contract failed; inspect only the sanitized boolean receipt.')
            raise SystemExit(1)
        LOG.info('Manual-session overwrite reproduced; the existing concurrent-refresh fix still passes.' if args.expect_baseline else 'Both native coordination contracts passed.')
    except Exception:
        LOG.error('Bounded fixture failed; no successful verification is claimed.')
        raise

if __name__ == '__main__':
    main()
