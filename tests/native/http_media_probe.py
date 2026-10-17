"""Native HTTP media-type and JSON-size contracts using local-only TLS/OAuth."""
from __future__ import annotations
import argparse
import hashlib
import json
import logging
from pathlib import Path
import sys
import tempfile
import threading

sys.dont_write_bytecode = True
import protocol_probe as protocol
from terminal_sse_probe import execute

LOG = logging.getLogger('http-media')
JSON_LIMIT = 16 * 1024 * 1024
CASES = {
    'json': ('application/json', False, True),
    'json_large': ('application/json', False, True),
    'json_upper': ('Application/JSON', False, True),
    'json_parameters': ('APPLICATION/JSON; Charset="utf-8"', False, True),
    'sse': ('text/event-stream', True, True),
    'sse_upper': ('Text/Event-Stream; charset=utf-8', True, True),
    'json_prefix': ('application/json-extra', False, False),
    'sse_prefix': ('text/event-stream-extra', True, False),
    'json_suffix': ('application/json+fixture', False, False),
    'combined': ('application/json, text/plain', False, False),
    'invalid_parameter': ('application/json; x="unterminated', False, False),
    'duplicate': ('application/json', False, False),
    'missing': (None, False, False),
    'oversized': ('application/json', False, False),
}

class Handler(protocol.Handler):
    def reply(self, value=None, status=200, kind='application/json'):
        case = self.server.case
        if case == 'prepare' or not isinstance(value, dict) or 'jsonrpc' not in value:
            return super().reply(value, status, kind)
        self.server.responses += 1
        media, sse, _ = CASES[case]
        body = json.dumps(value).encode()
        if sse:
            body = b'data: ' + body + b'\n\n'
        if case == 'json_large':
            body += b' ' * (2 * 1024 * 1024 - len(body))
        if case == 'oversized':
            body += b' ' * (JSON_LIMIT + 1 - len(body))
        self.server.response_size = len(body)
        self.send_response(status)
        if media is not None:
            self.send_header('Content-Type', media)
        if case == 'duplicate':
            self.send_header('Content-Type', 'text/plain')
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Connection', 'close')
        self.end_headers()
        try:
            self.wfile.write(body)
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError, OSError):
            LOG.debug('Connector rejected and closed an owned response.')
        finally:
            self.close_connection = True

class Fixture(protocol.Fixture):
    def __init__(self, ca, case):
        super().__init__(ca, case)
        self.RequestHandlerClass = Handler
        self.responses = 0
        self.response_size = 0


def run_case(binary, ca, output, mode, case):
    with tempfile.TemporaryDirectory(prefix='http-media-', dir=output) as tmp:
        root = Path(tmp)
        (root / 'config').mkdir()
        server = Fixture(ca, 'prepare' if mode == 'bridge' else case)
        worker = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .02}, daemon=True)
        worker.start()
        (root / 'config/connections.json').write_text(json.dumps({'version': 1, 'mode': 'proxy', 'selected': 'fixture',
            'profiles': [{'name': 'fixture', 'url': f'http://127.0.0.1:{server.server_port}'}]}), encoding='utf-8')
        try:
            if mode == 'bridge':
                prepared = execute(binary, root, ca, 'sign-in')
                if prepared['returncode'] != 0 or not prepared['completion_claimed']:
                    raise RuntimeError('Synthetic credential preparation failed.')
                server.case = case
            server.initialized = 0
            result = execute(binary, root, ca, mode)
            accepted = result['completion_claimed'] if mode == 'sign-in' else result['first_terminal_message'] == 'result'
            expected = CASES[case][2]
            passed = (accepted == expected and not result['controller_timeout'] and not result['private_marker_in_stderr'] and server.responses == 1)
            if mode == 'sign-in':
                passed &= result['callback_sent'] and (result['returncode'] == 0) == expected and server.initialized == int(expected)
            result.update(mode=mode, case=case, accepted=accepted, expected_success=expected,
                          initialized_sent=server.initialized, initialization_requests=server.responses,
                          response_bytes=server.response_size, json_limit=JSON_LIMIT, passed=bool(passed))
            LOG.info('Mode=%s case=%s accepted=%s contract_passed=%s', mode, case, accepted, passed)
            return result
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-baseline', action='store_true')
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] [http-media] %(message)s')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    names = ['json', 'json_large', 'json_upper', 'json_prefix', 'sse_upper', 'sse_prefix', 'oversized'] if args.expect_baseline else list(CASES)
    results = []
    LOG.debug('JSON has a distinct 16 MiB policy; the relay SSE limit remains 1 MiB.')
    with tempfile.TemporaryDirectory(prefix='http-media-certs-', dir=args.output.parent) as tmp:
        ca = Path(tmp)
        protocol.certificates(ca)
        (ca / 'empty').mkdir()
        for mode in ['sign-in', 'bridge']:
            for case in names:
                results.append(run_case(args.binary.resolve(), ca, args.output.parent, mode, case))
                args.output.write_text(json.dumps({'cases': len(results), 'passed': sum(row['passed'] for row in results),
                    'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'results': results}, indent=2) + '\n', encoding='utf-8')
    if args.expect_baseline:
        passed = all(not row['controller_timeout'] and row['initialization_requests'] == 1 and
            row['accepted'] == (row['case'] in {'json', 'json_large', 'json_prefix', 'sse_prefix'} or
                (row['case'] == 'oversized' and row['mode'] == 'bridge')) for row in results)
    else:
        passed = all(row['passed'] for row in results)
    LOG.warning('Synthetic response contracts do not certify every live-provider behavior.')
    if not passed:
        LOG.error('Native HTTP boundary contract failed; no retry hides this assertion.')
        raise SystemExit(1)
    LOG.info('Exact baseline HTTP violations reproduced.' if args.expect_baseline else 'All 28 native HTTP boundary cases passed.')

if __name__ == '__main__':
    try:
        main()
    except Exception:
        LOG.error('Bounded fixture failed; no private response contents are logged.')
        raise
