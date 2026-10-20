"""Native initialization error ordering: manual sign-in and normal stdio bridge.

All HTTP/TLS traffic terminates in a disposable loopback fixture. Real credentials,
external requests, global certificate trust, and marketplace writes are not used.
"""
from __future__ import annotations
import argparse
import hashlib
import http.client
import json
import logging
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time
import urllib.parse
import protocol_probe as probe

LOG = logging.getLogger('terminal-sse-probe')
ORIGINAL_REPLY = probe.Handler.reply
INITIALIZE = {'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {
    'protocolVersion': '2025-11-25', 'capabilities': {},
    'clientInfo': {'name': 'isolated-audit', 'version': '1'}}}
POSITIVE = {'json_success', 'success_then_error_sse'}


def terminal_kind(message):
    if not isinstance(message, dict) or message.get('jsonrpc') != '2.0':
        return None
    identity = message.get('id')
    if type(identity) not in (int, str) or identity not in (1, '1'):
        return None
    if sum(key in message for key in ('method', 'result', 'error')) != 1:
        return None
    error = message.get('error')
    if (isinstance(error, dict) and type(error.get('code')) is int
            and isinstance(error.get('message'), str)):
        return 'error'
    if isinstance(message.get('result'), dict):
        return 'result'
    return None


def bridge_message_kind(message):
    terminal = terminal_kind(message)
    if terminal is not None:
        return terminal
    if (isinstance(message, dict) and message.get('jsonrpc') == '2.0'
            and isinstance(message.get('method'), str)
            and not any(key in message for key in ('id', 'result', 'error'))):
        return 'notification'
    return 'invalid'


def representation_rejected(result):
    return (result['returncode'] == 1 and not result['controller_timeout']
            and not result['protocol_invalid'] and result['first_terminal_message'] is None
            and result['transport_rejected'])


def reply(self, value=None, status=200, kind='application/json'):
    if not isinstance(value, dict) or 'jsonrpc' not in value or self.server.case == 'prepare':
        return ORIGINAL_REPLY(self, value, status, kind)
    case = self.server.case
    error = {'jsonrpc': '2.0', 'id': 1, 'error': {'code': -32602, 'message': 'private-fixture-marker'}}
    if case == 'string_error_then_success_sse':
        error['id'] = '1'
    if case == 'json_error':
        return ORIGINAL_REPLY(self, error, 200, 'application/json')
    if case == 'json_success':
        return ORIGINAL_REPLY(self, value, 200, 'application/json')
    encode = lambda message: b'event: message\ndata: ' + json.dumps(message).encode() + b'\n\n'
    parts = [encode(error), encode(value)]
    if case == 'success_then_error_sse':
        parts.reverse()
    if case in {'error_only_closed_sse', 'error_only_open_sse'}:
        parts = parts[:1]
    self.send_response(200)
    self.send_header('Content-Type', 'text/event-stream')
    self.send_header('Connection', 'close')
    if case != 'error_only_open_sse':
        self.send_header('Content-Length', str(sum(map(len, parts))))
    self.end_headers()
    try:
        if case == 'split_error_then_success_sse':
            self.wfile.write(parts[0]); self.wfile.flush()
            self.server.stop_fixture.wait(0.25)
            self.wfile.write(parts[1]); self.wfile.flush()
        else:
            self.wfile.write(b''.join(parts)); self.wfile.flush()
        if case == 'error_only_open_sse':
            self.server.stop_fixture.wait(8)
    except (BrokenPipeError, ConnectionResetError):
        LOG.debug('Native process closed an owned fixture connection.')
    self.close_connection = True


def execute(binary: Path, root: Path, certificates: Path, command: str, ready_event: threading.Event | None = None) -> dict:
    env = os.environ.copy()
    env.update(SSL_CERT_FILE=str(certificates/'ca.pem'), SSL_CERT_DIR=str(certificates/'empty'))
    for key in ('TEMP', 'TMP', 'TMPDIR'):
        env[key] = str(root)
    previous_logs = set((root / 'logs').glob('*.log'))
    child = subprocess.Popen([str(binary), '--root', str(root), command], stdin=subprocess.PIPE,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", env=env)
    lines: queue.Queue[str] = queue.Queue()
    errors: list[str] = []
    reader_failures: list[bool] = []
    def collect(stream, receive) -> None:
        try:
            for line in stream:
                receive(line)
        except (UnicodeError, OSError):
            reader_failures.append(True)
    def read_output() -> None:
        collect(child.stdout, lines.put)
    def read_errors() -> None:
        collect(child.stderr, errors.append)
    readers = [threading.Thread(target=read_output, daemon=True), threading.Thread(target=read_errors, daemon=True)]
    for reader in readers:
        reader.start()
    observed = None
    protocol_invalid = False
    completion = False
    callback = False
    start = time.monotonic()
    timed_out = False
    try:
        if command == 'bridge':
            child.stdin.write(json.dumps(INITIALIZE)+'\n'); child.stdin.flush()
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            try:
                line = lines.get(timeout=0.05)
            except queue.Empty:
                if child.poll() is not None:
                    break
                continue
            if command == 'sign-in' and line.startswith('https://'):
                query = urllib.parse.parse_qs(urllib.parse.urlsplit(line.strip()).query)
                redirect = urllib.parse.urlsplit(query['redirect_uri'][0])
                assert redirect.hostname in {'127.0.0.1', 'localhost'}
                connection = http.client.HTTPConnection(redirect.hostname, redirect.port, timeout=2)
                callback_query = urllib.parse.urlencode({'code': 'fixture-code', 'state': query['state'][0], 'iss': probe.ISSUER})
                connection.request('GET', redirect.path + '?' + callback_query)
                response = connection.getresponse(); response.read(); connection.close()
                callback = True
            completion |= 'Sign-in complete.' in line
            if command == 'bridge':
                try:
                    message = json.loads(line)
                except json.JSONDecodeError:
                    protocol_invalid = True
                    break
                kind = bridge_message_kind(message)
                if kind == 'invalid':
                    protocol_invalid = True
                    break
                if kind in ('result', 'error'):
                    observed = kind
                    if observed == 'result' and ready_event is not None:
                        child.stdin.write(json.dumps({'jsonrpc':'2.0','method':'notifications/initialized'})+'\n')
                        child.stdin.flush()
                        if not ready_event.wait(max(0, deadline-time.monotonic())):
                            timed_out = True
                    break
        if command == 'bridge':
            child.stdin.close()
        try:
            child.wait(timeout=1 if command == 'sign-in' else 3)
        except subprocess.TimeoutExpired:
            timed_out = True; child.kill(); child.wait(timeout=2)
        for reader in readers:
            reader.join(timeout=1)
        protocol_invalid |= bool(reader_failures) or any(reader.is_alive() for reader in readers)
        while not lines.empty():
            line = lines.get_nowait()
            completion |= 'Sign-in complete.' in line
            if command == 'bridge':
                try:
                    kind = bridge_message_kind(json.loads(line))
                except json.JSONDecodeError:
                    kind = 'invalid'
                if kind in ('result', 'error') and observed is None:
                    observed = kind
                elif kind != 'notification':
                    protocol_invalid = True
        transport_rejected = False
        for logfile in set((root / 'logs').glob('*.log')) - previous_logs:
            for entry in logfile.read_text(encoding='utf-8').splitlines():
                if ('MCP HTTP failure stage=post class=unexpected_http_response ' in entry
                        or 'MCP HTTP failure stage=post class=unexpected_content_type ' in entry):
                    transport_rejected = True
        return {'returncode': child.returncode, 'callback_sent': callback,
                'protocol_invalid': protocol_invalid, 'transport_rejected': transport_rejected,
                'first_terminal_message': observed, 'completion_claimed': completion,
                'controller_timeout': timed_out, 'elapsed_seconds': round(time.monotonic()-start, 3),
                'private_marker_in_stderr': any('private-fixture-marker' in line for line in errors)}
    finally:
        if child.poll() is None:
            child.kill(); child.wait(timeout=2)
        for reader in readers:
            reader.join(timeout=1)
        for stream in (child.stdin, child.stdout, child.stderr):
            if stream and not stream.closed:
                stream.close()


def run(binary: Path, ca: Path, mode: str, case: str, output_dir: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix='terminal-case-', dir=output_dir) as temporary:
        root = Path(temporary); (root/'config').mkdir()
        server = probe.Fixture(ca, 'prepare' if mode == 'bridge' else case)
        server.stop_fixture = threading.Event()
        thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .02}, daemon=True)
        thread.start()
        (root/'config/connections.json').write_text(json.dumps({'version': 1, 'mode': 'proxy',
            'selected': 'fixture', 'profiles': [{'name': 'fixture', 'url': f'http://127.0.0.1:{server.server_port}'}]}),encoding='utf-8')
        preparation = None
        try:
            if mode == 'bridge':
                preparation = execute(binary, root, ca, 'sign-in')
                assert preparation['returncode'] == 0 and preparation['completion_claimed'], 'Synthetic credential preparation failed.'
                server.case = case
            server.initialized = 0
            result = execute(binary, root, ca, mode)
            positive = case in POSITIVE
            if mode == 'sign-in':
                passed = (result['callback_sent'] and not result['controller_timeout']
                          and (result['returncode'] == 0) == positive
                          and result['completion_claimed'] == positive
                          and server.initialized == int(positive))
            else:
                passed = (not result['controller_timeout']
                          and result['first_terminal_message'] == ('result' if positive else 'error')
                          and (not positive or result['returncode'] == 0))
            passed = passed and not result['private_marker_in_stderr'] and not result['protocol_invalid']
            result.update(case=case, mode=mode, initialized_sent=server.initialized,
                          expected_success=positive, passed=passed,
                          preparation_completed=preparation is not None)
            LOG.log(logging.INFO if passed else logging.ERROR,
                    'Mode=%s case=%s terminal=%s completion=%s ready=%d passed=%s',
                    mode, case, result['first_terminal_message'], result['completion_claimed'], server.initialized, passed)
            return result
        finally:
            server.stop_fixture.set()
            server.shutdown(); server.server_close(); thread.join(timeout=1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--mode', choices=['sign-in', 'bridge'], required=True)
    parser.add_argument('--cases', nargs='+', default=['json_success', 'json_error',
        'error_then_success_sse', 'split_error_then_success_sse', 'string_error_then_success_sse',
        'error_only_closed_sse', 'error_only_open_sse', 'success_then_error_sse'])
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] %(message)s')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    probe.Handler.reply = reply
    results = []
    binary = args.binary.resolve()
    binary_digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    try:
        with tempfile.TemporaryDirectory(prefix='terminal-certs-', dir=args.output.parent) as temporary:
            ca = Path(temporary); probe.certificates(ca); (ca/'empty').mkdir()
            for case in args.cases:
                results.append(run(binary, ca, args.mode, case, args.output.parent))
                receipt = {'binary_sha256': binary_digest,
                           'mode': args.mode, 'cases': len(results), 'passed': sum(r['passed'] for r in results), 'results': results}
                args.output.write_text(json.dumps(receipt, indent=2)+'\n', encoding='utf-8')
    finally:
        probe.Handler.reply = ORIGINAL_REPLY
    LOG.warning('No live account or external network was used; failing baseline cases remain failures.')
    LOG.info('Completed %d cases; %d passed and %d failed.', len(results), sum(r['passed'] for r in results), sum(not r['passed'] for r in results))
    if not all(r['passed'] for r in results):
        raise SystemExit(1)

if __name__ == '__main__':
    try:
        main()
    except Exception:
        LOG.error('Terminal-order probe aborted; no unexecuted case is counted.')
        raise
