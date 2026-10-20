"""Recheck raw envelope and framing boundaries with isolated native TLS fixtures."""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import logging
from pathlib import Path
import tempfile
import protocol_probe as probe

LOG = logging.getLogger('extended-protocol-probe')
ORIGINAL_REPLY = probe.Handler.reply
POSITIVE = {'valid_json_control', 'valid_sse_lf', 'valid_sse_crlf', 'valid_sse_comments'}


def extended_reply(self, value=None, status=200, kind='application/json'):
    if not isinstance(value, dict) or 'jsonrpc' not in value:
        return ORIGINAL_REPLY(self, value, status, kind)
    case = self.server.case
    valid = copy.deepcopy(value)
    if case.startswith('mixed_error_null'):
        value['error'] = None
    elif case.startswith('mixed_error'):
        value['error'] = {'code': -32602, 'message': 'private-fixture-marker'}
    elif case.startswith('mixed_method'):
        value['method'] = 'ping'
    elif case.startswith('invalid_jsonrpc'):
        value['jsonrpc'] = '1.0'
    elif case.startswith('top_array'):
        value = [value]
    elif case == 'error_then_valid_sse':
        value = {'jsonrpc': '2.0', 'id': 1, 'error': {'code': -32602, 'message': 'private-fixture-marker'}}
    elif case == 'invalid_version_then_valid_sse':
        value['result']['protocolVersion'] = '2099-01-01'
    payload = json.dumps(value, separators=(',', ':')).encode()
    if case.startswith('invalid_utf8'):
        payload = b'\xff' + payload
    if case == 'malformed_then_valid_sse':
        payload = b'{"private-fixture-marker":'
    if case == 'oversize_json':
        value['result']['instructions'] = 'x' * (16 * 1024 * 1024)
        payload = json.dumps(value).encode()
    if case.endswith('_sse') or case.startswith('valid_sse'):
        delimiter = b'\r\n' if case == 'valid_sse_crlf' else b'\n'
        payload = b'event: message' + delimiter + b'data: ' + payload + delimiter * 2
        if case.endswith('then_valid_sse'):
            payload += b'data: ' + json.dumps(valid).encode() + delimiter * 2
        if case == 'valid_sse_comments':
            payload = (b': heartbeat\n' * 1000) + b'\n' + payload
        kind = 'text/event-stream'
    else:
        kind = 'application/json'
    self.send_response(status)
    self.send_header('Content-Type', kind)
    self.send_header('Content-Length', str(len(payload)))
    self.send_header('Connection', 'close')
    self.end_headers()
    try:
        self.wfile.write(payload)
        self.wfile.flush()
    except (BrokenPipeError, ConnectionResetError):
        LOG.debug('Native client closed a rejected fixture response.')
    self.close_connection = True


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cases', nargs='+', required=True)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] %(message)s')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    probe.Handler.reply = extended_reply
    results = []
    binary_digest = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    try:
        with tempfile.TemporaryDirectory(prefix='extended-certs-', dir=args.output.parent) as temporary:
            root = Path(temporary)
            probe.certificates(root)
            for case in args.cases:
                result = probe.run_case(args.binary.resolve(), root, case, args.output.parent)
                positive = case in POSITIVE
                result['expected_success'] = positive
                result['passed'] = (
                    result['callback_sent'] and not result['timeout']
                    and (result['returncode'] == 0) == positive
                    and result['completion_claimed'] == positive
                    and result['initialized_sent'] == (1 if positive else 0)
                )
                results.append(result)
                if not result['passed']:
                    LOG.error('Raw-envelope contract failed for case %s.', case)
                # Persist only sanitized receipts after each case.
                receipt = {'binary_sha256': binary_digest,
                           'cases': len(results), 'passed': sum(r['passed'] for r in results),
                           'results': results}
                args.output.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
        LOG.warning('These tests use synthetic identities and do not certify a live Upwork account.')
        LOG.info('Completed %d cases; %d passed.', len(results), sum(r['passed'] for r in results))
    finally:
        probe.Handler.reply = ORIGINAL_REPLY
    if not all(r['passed'] for r in results):
        raise SystemExit(1)

if __name__ == '__main__':
    try:
        main()
    except Exception:
        LOG.error('Extended probe aborted; no unexecuted case is counted as passed.')
        raise
