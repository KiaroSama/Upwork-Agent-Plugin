"""Packaged ordinary POST/GET contracts; synthetic loopback TLS only."""
import argparse
import hashlib
import json
import logging
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse

sys.dont_write_bytecode = True
import protocol_probe as protocol
from terminal_sse_probe import execute, INITIALIZE, terminal_kind

LOG = logging.getLogger('ordinary-http')
LIMIT = 16 * 1024 * 1024


class Handler(protocol.Handler):
    def do_GET(self):
        if urllib.parse.urlsplit(self.path).path != '/mcp' or self.server.case == 'prepare':
            return super().do_GET()
        self.server.gets += 1
        data = b'data: {"jsonrpc":"2.0","id":"ordinary-get-ping","method":"ping"}\n\n'
        self.send_response(200)
        self.send_header('Content-Type', 'Text/Event-Stream; charset=utf-8')
        self.send_header('Connection', 'close')
        self.end_headers()
        try:
            self.wfile.write(data)
            self.wfile.flush()
            self.server.stop_fixture.wait(10)
        except (BrokenPipeError, ConnectionResetError, OSError):
            LOG.debug('Owned listening GET closed.')
        self.close_connection = True

    def do_POST(self):
        if urllib.parse.urlsplit(self.path).path != '/mcp' or self.server.case == 'prepare':
            return super().do_POST()
        request = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
        method = request.get('method')
        if method == 'initialize':
            self.server.initializations += 1
            body = {'jsonrpc': '2.0', 'id': request['id'], 'result': {
                'protocolVersion': '2025-11-25', 'capabilities': {'tools': {}},
                'serverInfo': {'name': 'ordinary-fixture', 'version': '1'}}}
            data = json.dumps(body).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Mcp-Session-Id', 'ordinary-fixture-session')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            self.wfile.flush()
            return
        if method == 'notifications/initialized':
            self.server.initialized += 1
            return self.reply(None, 202)
        if method is None:
            if request.get('id') == 'ordinary-get-ping' and request.get('result') == {}:
                self.server.control.set()
            return self.reply(None, 202)
        assert method == 'tools/list'
        self.server.ordinary += 1
        data = json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': {'tools': []}}).encode()
        case = self.server.case
        if case == 'large_json':
            data += b' ' * (2 * 1024 * 1024 - len(data))
        if case == 'chunked_overflow':
            data += b' ' * (LIMIT + 1 - len(data))
        self.send_response(200)
        self.send_header('Content-Type', 'application/json-extra' if case == 'wrong_media' else 'Application/JSON')
        self.send_header('Connection', 'close')
        if case == 'chunked_overflow':
            self.send_header('Transfer-Encoding', 'chunked')
        else:
            self.send_header('Content-Length', str(len(data) + (100 if case == 'truncated' else 0)))
        self.end_headers()
        try:
            if case == 'chunked_overflow':
                for offset in range(0, len(data), 65536):
                    part = data[offset:offset + 65536]
                    self.wfile.write(f'{len(part):x}\r\n'.encode() + part + b'\r\n')
                self.wfile.write(b'0\r\n\r\n')
            else:
                self.wfile.write(data)
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError, OSError):
            LOG.debug('Owned rejected response closed.')
        self.close_connection = True


class Fixture(protocol.Fixture):
    def __init__(self, ca):
        super().__init__(ca, 'prepare')
        self.RequestHandlerClass = Handler
        self.gets = self.ordinary = self.initializations = 0
        self.control = threading.Event()
        self.stop_fixture = threading.Event()


def run(binary, ca, output, case):
    with tempfile.TemporaryDirectory(prefix='ordinary-http-', dir=output) as temporary:
        root = Path(temporary)
        (root / 'config').mkdir()
        server = Fixture(ca)
        worker = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .02}, daemon=True)
        worker.start()
        (root / 'config/connections.json').write_text(json.dumps({'version': 1, 'mode': 'proxy',
            'selected': 'fixture', 'profiles': [{'name': 'fixture', 'url': f'http://127.0.0.1:{server.server_port}'}]}), encoding='utf-8')
        child = None
        readers = []
        try:
            prepared = execute(binary, root, ca, 'sign-in')
            assert prepared['returncode'] == 0 and prepared['completion_claimed']
            server.case = case
            server.initialized = 0
            env = os.environ.copy()
            env.update(SSL_CERT_FILE=str(ca / 'ca.pem'), SSL_CERT_DIR=str(ca / 'empty'))
            env.update({key: str(root) for key in ('TEMP', 'TMP', 'TMPDIR')})
            child = subprocess.Popen([str(binary), '--root', str(root), 'bridge'],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                text=True, encoding='utf-8', env=env)
            lines = queue.Queue()
            errors = []
            reader_failures = []
            def collect(stream, target, terminal=False):
                try:
                    for line in stream:
                        target(line)
                except (UnicodeError, OSError):
                    reader_failures.append(True)
                finally:
                    if terminal:
                        target(None)
            readers = [threading.Thread(target=collect, args=(child.stdout, lines.put, True), daemon=True),
                       threading.Thread(target=collect, args=(child.stderr, errors.append), daemon=True)]
            for reader in readers:
                reader.start()
            deadline = time.monotonic() + 8
            def send(message):
                child.stdin.write(json.dumps(message) + '\n')
                child.stdin.flush()
            send(INITIALIZE)
            initialized = False
            terminal = None
            while time.monotonic() < deadline:
                if terminal and server.control.is_set():
                    break
                try:
                    line = lines.get(timeout=.05)
                except queue.Empty:
                    continue
                if line is None:
                    break
                message = json.loads(line)
                assert isinstance(message, dict), 'Protocol message must be an object'
                if message.get('method') == 'ping':
                    assert message.get('jsonrpc') == '2.0' and 'result' not in message and 'error' not in message
                    assert message['id'] == 'ordinary-get-ping'
                    send({'jsonrpc': '2.0', 'id': message['id'], 'result': {}})
                elif not initialized:
                    assert terminal_kind(message) == 'result'
                    initialized = True
                    send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
                    send({'jsonrpc': '2.0', 'id': 2, 'method': 'tools/list'})
                elif type(message.get('id')) is int and message.get('id') == 2:
                    assert terminal is None, 'Duplicate ordinary terminal response'
                    assert type(message.get('id')) is int
                    assert message.get('jsonrpc') == '2.0' and 'error' not in message and 'method' not in message
                    assert message.get('result') == {'tools': []}
                    terminal = 'result'
                else:
                    assert (message.get('jsonrpc') == '2.0' and isinstance(message.get('method'), str)
                            and not any(key in message for key in ('id', 'result', 'error'))), 'Unexpected ordinary protocol message'
                if terminal and server.control.is_set():
                    break
            positive = case in ('json', 'large_json')
            if positive:
                assert terminal == 'result' and server.gets >= 1 and server.control.is_set()
                child.stdin.close()
                child.wait(timeout=3)
                assert child.returncode == 0
            else:
                assert terminal is None
                child.wait(timeout=3)
                assert child.returncode == 1
                diagnostics = ''.join(path.read_text(encoding='utf-8') for path in (root / 'logs').glob('*.log'))
                expected = 'class=response_body' if case == 'truncated' else 'class=unexpected_content_type' if case == 'wrong_media' else 'class=unexpected_http_response'
                assert expected in diagnostics
            for reader in readers:
                reader.join(timeout=1)
            assert not reader_failures and not any(reader.is_alive() for reader in readers), 'Protocol output did not drain cleanly'
            while not lines.empty():
                line = lines.get_nowait()
                if line is None:
                    continue
                message = json.loads(line)
                assert isinstance(message, dict) and message.get('jsonrpc') == '2.0'
                if message.get('method') == 'ping':
                    assert message.get('id') == 'ordinary-get-ping' and 'result' not in message and 'error' not in message
                else:
                    assert (isinstance(message.get('method'), str)
                            and not any(key in message for key in ('id', 'result', 'error'))), 'Unexpected or duplicate terminal output after exit'
            assert initialized and server.initializations == 1 and server.ordinary == 1 and server.initialized == 1
            assert not any('fixture-session' in line or 'disposable-fixture-token' in line for line in errors)
            return {'case': case, 'passed': True, 'ordinary_posts': server.ordinary,
                    'initializations': server.initializations, 'listening_gets': server.gets,
                    'get_control_replied': server.control.is_set(), 'exit': child.returncode, 'reaped': True}
        finally:
            if child and child.poll() is None:
                child.kill()
                child.wait(timeout=3)
            for reader in readers:
                reader.join(timeout=1)
            if child:
                for stream in (child.stdin, child.stdout, child.stderr):
                    if stream and not stream.closed:
                        stream.close()
            server.stop_fixture.set()
            server.shutdown()
            server.server_close()
            worker.join(timeout=1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve()
    binary_digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    rows = []
    with tempfile.TemporaryDirectory(prefix='ordinary-certs-', dir=args.output.parent) as temporary:
        ca = Path(temporary)
        protocol.certificates(ca)
        (ca / 'empty').mkdir()
        for case in ('json', 'large_json', 'wrong_media', 'chunked_overflow', 'truncated'):
            rows.append(run(binary, ca, args.output.parent, case))
            args.output.write_text(json.dumps({'binary_sha256': binary_digest, 'results': rows}, indent=2) + '\n', encoding='utf-8')
            LOG.info('Ordinary HTTP case=%s passed.', case)


if __name__ == '__main__':
    main()
