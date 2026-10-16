"""Real-process, gated initialization pings; all HTTP/TLS terminates locally."""
from __future__ import annotations
import argparse
import json
import logging
import tempfile
import threading
import urllib.parse
from pathlib import Path
import sys

# Direct invocation must not emit sibling bytecode into the source tree.
sys.dont_write_bytecode = True
import protocol_probe as protocol
from terminal_sse_probe import execute

LOG = logging.getLogger('initialization-ping')

class Handler(protocol.Handler):
    def do_POST(self):
        if urllib.parse.urlsplit(self.path).path != '/mcp' or self.server.case == 'prepare':
            return super().do_POST()
        size = int(self.headers.get('Content-Length', '0'))
        if not 0 < size <= 1024 * 1024:
            return self.reply(None, 400)
        request = json.loads(self.rfile.read(size))
        method = request.get('method')
        if method is None:
            self.server.ping_replies += 1
            expected = self.server.expected
            accepted = (request.get('id') == expected and type(request.get('id')) is type(expected)
                        and request.get('result') == {}
                        and self.headers.get('Mcp-Session-Id') == 'gated-fixture'
                        and self.headers.get('Authorization') == 'Bearer disposable-fixture-token'
                        and (self.headers.get('Mcp-Protocol-Version') in (None, '2025-03-26')
                             if self.server.case == 'fallback' else self.headers.get('Mcp-Protocol-Version') is None))
            self.server.valid_replies &= accepted
            self.reply(None, 500 if self.server.case == 'reply_failure' else 202 if accepted else 400)
            self.server.answered.set()
            return
        if method == 'notifications/initialized':
            negotiated = '2025-03-26' if self.server.case == 'fallback' else '2025-11-25'
            self.server.ready_version_ok &= self.headers.get('Mcp-Protocol-Version') == negotiated
            self.server.initialized += 1
            self.server.ready_seen.set()
            return self.reply(None, 202)
        if method != 'initialize':
            return self.reply(None, 400)
        self.server.initialize_requests += 1
        terminal = {'jsonrpc':'2.0','id':request['id'],'result':{
            'protocolVersion':'2025-03-26' if self.server.case == 'fallback' else '2025-11-25',
            'capabilities':{},'serverInfo':{'name':'gated','version':'1'}}}
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Mcp-Session-Id', 'gated-fixture')
        self.send_header('Connection', 'close')
        self.end_headers()
        def emit(value):
            self.wfile.write(b'event: message\ndata: '+json.dumps(value).encode()+b'\n\n')
            self.wfile.flush()
        case = self.server.case
        identifiers = {'numeric':[7], 'string':['ping-a'], 'numeric_string':['007'],
                       'multiple':[7,'7'], 'overflow':list(range(17)), 'no_ping':[]}.get(case, ['ping-a'])
        try:
            if case == 'log_first':
                emit({'jsonrpc':'2.0','method':'notifications/message','params':{'level':'info','data':'fixture'}})
            for identifier in identifiers:
                self.server.answered.clear()
                self.server.expected = identifier
                emit({'jsonrpc':'2.0','id':identifier,'method':'ping'})
                if not self.server.answered.wait(1.5):
                    self.server.gate_expired = True
                    return
                if not self.server.valid_replies or case == 'reply_failure':
                    return
            if case == 'ping_then_error':
                emit({'jsonrpc':'2.0','id':request['id'],'error':{'code':-32602,'message':'private-fixture-marker'}})
            emit(terminal)
        except (BrokenPipeError, ConnectionResetError, OSError):
            LOG.debug('Connector closed a bounded fixture connection.')
        finally:
            self.close_connection = True

class Fixture(protocol.Fixture):
    def __init__(self, root, case):
        super().__init__(root, case)
        self.RequestHandlerClass = Handler
        self.answered = threading.Event()
        self.expected = None
        self.valid_replies = True
        self.gate_expired = False
        self.initialize_requests = 0
        self.ready_version_ok = True
        self.ready_seen = threading.Event()

def run(binary, ca, output, mode, case):
    with tempfile.TemporaryDirectory(prefix='ping-case-', dir=output) as directory:
        root = Path(directory)
        (root/'config').mkdir()
        server = Fixture(ca, 'prepare' if mode == 'bridge' else case)
        worker = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .02}, daemon=True)
        worker.start()
        (root/'config/connections.json').write_text(json.dumps({'version':1,'mode':'proxy','selected':'fixture',
            'profiles':[{'name':'fixture','url':f'http://127.0.0.1:{server.server_port}'}]}), encoding='utf-8')
        try:
            if mode == 'bridge':
                preparation = execute(binary, root, ca, 'sign-in')
                if not preparation['completion_claimed'] or preparation['returncode'] != 0:
                    raise RuntimeError('Synthetic credential preparation failed.')
                server.case = case
            server.initialized = 0
            server.ready_seen.clear()
            result = execute(binary, root, ca, mode, server.ready_seen if mode == "bridge" else None)
            positive = case not in {'reply_failure','overflow','ping_then_error'}
            expected_pings = 0 if case == 'no_ping' else 2 if case == 'multiple' else 16 if case == 'overflow' else 1
            application_ok = (result['completion_claimed'] if mode == 'sign-in'
                              else result['first_terminal_message'] == 'result')
            passed = (application_ok == positive and server.valid_replies
                      and server.ping_replies == expected_pings and server.initialize_requests == 1
                      and not result['controller_timeout'] and not result['private_marker_in_stderr']
                      and server.initialized == int(positive)
                      and server.ready_version_ok
                      and server.ready_seen.is_set() == positive)
            if mode == 'sign-in':
                passed &= result['callback_sent'] and server.initialized == int(positive) and (result['returncode'] == 0) == positive
            if case == 'ping_then_error' and mode == 'bridge':
                passed &= result['first_terminal_message'] == 'error'
            result.update(mode=mode, case=case, ping_replies=server.ping_replies,
                          valid_reply_ids_and_headers=server.valid_replies,
                          initialize_requests=server.initialize_requests, initialized_sent=server.initialized,
                          gate_expired=server.gate_expired, negotiated_ready_header_valid=server.ready_version_ok,
                          passed=bool(passed))
            LOG.info('Mode=%s case=%s ping_replies=%s passed=%s', mode, case, server.ping_replies, passed)
            return result
        finally:
            server.answered.set()
            server.shutdown()
            server.server_close()
            worker.join(timeout=1)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-baseline', action='store_true')
    parser.add_argument('--expect-offered-version', action='store_true')
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] [initialization-ping] %(message)s')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    cases = ['fallback','no_ping'] if args.expect_offered_version else ['numeric','no_ping'] if args.expect_baseline else ['numeric','string','numeric_string','multiple','log_first','no_ping','ping_then_error','reply_failure','overflow','fallback']
    results = []
    with tempfile.TemporaryDirectory(prefix='ping-certs-', dir=args.output.parent) as directory:
        ca = Path(directory)
        protocol.certificates(ca)
        (ca/'empty').mkdir()
        for mode in ['sign-in','bridge']:
            for case in cases:
                results.append(run(args.binary.resolve(), ca, args.output.parent, mode, case))
                args.output.write_text(json.dumps({'cases':len(results),'passed':sum(row['passed'] for row in results),'results':results}, indent=2)+'\n', encoding='utf-8')
    LOG.warning('This fixture is not a live Upwork compatibility test.')
    accepted = all((not row['passed'] and row['gate_expired'] and row['ping_replies'] == 0)
                   if args.expect_baseline and row['case'] == 'numeric' else row['passed'] for row in results)
    if args.expect_offered_version:
        accepted = all((not row['passed'] and not row['valid_reply_ids_and_headers']
                        and row['ping_replies'] == 1 and not row['controller_timeout'])
                       if row['case'] == 'fallback' else row['passed'] for row in results)
    if not accepted:
        LOG.error('Gated initialization contract failed; no unexecuted case is counted.')
        raise SystemExit(1)
    LOG.info('Offered-version incompatibility reproduced in both paths.' if args.expect_offered_version
             else 'Baseline ping defect reproduced in both paths.' if args.expect_baseline
             else 'All gated native ping contracts passed.')

if __name__ == '__main__':
    try:
        main()
    except Exception:
        LOG.error('Bounded ping fixture aborted; private values suppressed.')
        raise
