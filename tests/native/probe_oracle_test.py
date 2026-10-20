"""Pure controller regressions: no native build, account or network required."""
import ast
import hashlib
import io
import json
import subprocess
import sys
import tempfile
import threading
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import terminal_sse_probe as terminal
from terminal_sse_probe import terminal_kind, bridge_message_kind, representation_rejected


def lifecycle_controls():
    class InvalidText(io.StringIO):
        def __iter__(self):
            raise UnicodeDecodeError('utf-8', b'\xff', 0, 1, 'fixture')

    class Child:
        def __init__(self, output, code=0, timeout=False):
            self.stdin = io.StringIO()
            self.stdout = InvalidText() if output is None else io.StringIO(output)
            self.stderr = io.StringIO()
            self.returncode = None
            self.code, self.timeout, self.killed = code, timeout, False
        def poll(self):
            return self.returncode
        def wait(self, timeout):
            if self.timeout and not self.killed:
                raise subprocess.TimeoutExpired('fixture', timeout)
            self.returncode = self.code
            return self.returncode
        def kill(self):
            self.killed = True
            self.code = -9

    result = '{"jsonrpc":"2.0","id":1,"result":{}}\n'
    cases = [
        ('success', result, 0, False, 'result', False),
        ('server_error', '{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"fixture"}}\n', 0, False, 'error', False),
        ('no_output', '', 1, False, None, False),
        ('wrong_id', result.replace('"id":1', '"id":99'), 1, False, None, True),
        ('notification', '{"jsonrpc":"2.0","method":"notice"}\n' + result, 0, False, 'result', False),
        ('mixed', '{"jsonrpc":"2.0","id":1,"result":{},"error":{}}\n', 1, False, None, True),
        ('malformed', 'not-json\n', 1, False, None, True),
        ('duplicate', result + result, 0, False, 'result', True),
        ('unrelated_crash', '', -9, False, None, False),
        ('timeout', result, 0, True, 'result', False),
        ('invalid_utf8', None, 1, False, None, True),
    ]
    # Immediate fake deadlines keep lifecycle sensitivity below the native/network tier.
    work = Path(__file__).resolve().parents[2] / '.ci-work'
    work.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='oracle-', dir=work) as temporary:
        root = Path(temporary)
        for name, output, code, timeout, observed, invalid in cases:
            child = Child(output, code, timeout)
            before = set(threading.enumerate())
            with patch.object(terminal.subprocess, 'Popen', return_value=child), \
                    patch.object(terminal.time, 'monotonic', side_effect=[0, 0, 5, 5]):
                actual = terminal.execute(Path('fixture'), root, root, 'bridge')
            assert actual['first_terminal_message'] == observed, name
            assert actual['protocol_invalid'] == invalid, name
            assert actual['controller_timeout'] == timeout, name
            assert not representation_rejected(actual), name
            assert child.returncode == (-9 if timeout else code) and child.killed == timeout, name
            assert actual['returncode'] == child.returncode, name
            assert all(stream.closed for stream in (child.stdin, child.stdout, child.stderr)), name
            assert not (set(threading.enumerate()) - before), name


def digest_controls():
    class Input:
        def __init__(self):
            self.reads = 0
        def read_bytes(self):
            self.reads += 1
            return b'fixture executable'
    for script in ('extended_protocol_probe.py', 'http_media_probe.py', 'terminal_sse_probe.py', 'ordinary_http_probe.py', 'run.py'):
        source = ast.parse(Path(__file__).with_name(script).read_text(encoding='utf-8'))
        function = next(node for node in source.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
        assignment = next(node for node in function.body if isinstance(node, ast.Assign)
                          and any(isinstance(target, ast.Name) and target.id == 'binary_digest' for target in node.targets))
        fixture = Input()
        class Args:
            binary = fixture
        context = {'hashlib': hashlib, 'binary': fixture, 'args': Args()}
        exec(compile(ast.Module(body=[assignment], type_ignores=[]), script, 'exec'), context)
        # N receipt serializations reuse the production digest assignment, without rereading input.
        receipts = [json.loads(json.dumps({'binary_sha256': context['binary_digest']})) for _ in range(28)]
        assert fixture.reads == 1
        assert all(row['binary_sha256'] == hashlib.sha256(b'fixture executable').hexdigest() for row in receipts)
        assert all(row['binary_sha256'] != hashlib.sha256(b'wrong executable').hexdigest() for row in receipts)


def main():
    digest_controls()
    lifecycle_controls()
    for value in [None, [], {}, {'jsonrpc': '2.0', 'method': 'notice'},
                  {'jsonrpc': '2.0', 'id': 99, 'result': {}},
                  {'jsonrpc': '1.0', 'id': 1, 'result': {}},
                  {'jsonrpc': '2.0', 'id': True, 'result': {}},
                  {'jsonrpc': '2.0', 'id': 1, 'result': {}, 'error': {}},
                  {'jsonrpc': '2.0', 'id': 1, 'method': 'ping', 'result': {}}]:
        assert terminal_kind(value) is None
    for identity in [1, '1']:
        assert terminal_kind({'jsonrpc': '2.0', 'id': identity, 'result': {}}) == 'result'
        assert terminal_kind({'jsonrpc': '2.0', 'id': identity, 'error': {'code': -1, 'message': 'fixture'}}) == 'error'
    for error in [{}, {'code': True, 'message': 'fixture'}, {'code': -1}, {'code': -1, 'message': 3}]:
        assert bridge_message_kind({'jsonrpc': '2.0', 'id': 1, 'error': error}) == 'invalid'
    assert bridge_message_kind({'jsonrpc': '2.0', 'method': 'notifications/progress'}) == 'notification'
    for malformed in [{'method': 'notice'}, {'jsonrpc': '1.0', 'method': 'notice'},
                      {'jsonrpc': '2.0', 'id': 3, 'method': 'ping'},
                      {'jsonrpc': '2.0', 'id': 1, 'result': None}]:
        assert bridge_message_kind(malformed) == 'invalid'
    base = {'returncode': 1, 'controller_timeout': False, 'protocol_invalid': False,
            'first_terminal_message': None, 'transport_rejected': True}
    assert representation_rejected(base)
    for key, value in [('returncode', 0), ('returncode', -9),
                       ('controller_timeout', True), ('protocol_invalid', True), ('transport_rejected', False),
                       ('first_terminal_message', 'result')]:
        assert not representation_rejected(dict(base, **{key: value}))
    if '--output' in sys.argv:
        output = Path(sys.argv[sys.argv.index('--output') + 1])
        output.write_text(json.dumps({'passed': True, 'tier': 'pure-controller',
            'native_executed': False, 'controls': ['identity', 'envelope', 'causal-rejection', 'digest-reuse', 'fake-child-lifecycle']}) + '\n', encoding='utf-8')
    print('Controller causal-oracle controls passed.')


if __name__ == '__main__':
    main()
