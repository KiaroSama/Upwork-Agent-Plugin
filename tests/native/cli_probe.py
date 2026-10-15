"""Bounded offline CLI regression checks using only isolated synthetic state."""
from __future__ import annotations
import argparse
import hashlib
import json
import logging
import os
from pathlib import Path
import subprocess
import tempfile

LOG = logging.getLogger('cli-probe')

def main() -> None:
    parser=argparse.ArgumentParser(); parser.add_argument('--binary',type=Path,required=True); parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args(); binary=args.binary.resolve()
    logging.basicConfig(level=logging.INFO,format='%(asctime)s [%(levelname)s] %(message)s')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    receipts=[]; processes=0
    def check(name: str, condition: bool) -> None:
        receipts.append({'case':name,'passed':bool(condition)})
        if not condition:LOG.error('Assertion failed: %s',name)
    def call(root: Path, *command: str, data: str='') -> subprocess.CompletedProcess:
        nonlocal processes
        env=os.environ.copy()
        for key in ('TEMP','TMP','TMPDIR'):env[key]=str(root)
        processes+=1
        return subprocess.run([str(binary),'--root',str(root),*command],input=data.encode(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=4,env=env)
    def snapshot(root: Path) -> dict:
        return {str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file() and not p.is_symlink()}
    LOG.debug('All child processes are bounded and all fixture inputs are synthetic.')
    with tempfile.TemporaryDirectory(prefix='cli-probe-',dir=args.output.parent) as tmp:
        work=Path(tmp)
        # Invalid commands must have no startup side effects in a pristine root.
        invalid=[['direct','--help'],['direct','extra'],['block','extra'],['bridge','extra'],['sign-in','extra'],['menu','extra'],['status','extra'],['preview','extra'],['desktop-config','extra'],['register','extra'],['--version','extra'],['select'],['select','missing','extra'],['import'],['import','missing','extra'],['add','extra'],['local-proxy','extra'],['local-settings','wrong'],['local-settings','set','extra'],['cleanup','unexpected'],['cleanup','--apply','extra'],['unknown'],['--root'],['--help']]
        for index,command in enumerate(invalid):
            root=work/f'invalid-{index}';root.mkdir()
            result=call(root,*command)
            check(f'invalid-{index}-fails',result.returncode!=0)
            check(f'invalid-{index}-no-files',not list(root.iterdir()))
        root=work/'state';root.mkdir()
        check('blocked-default',json.loads(call(root,'status').stdout)['mode']=='blocked')
        b=call(root,'bridge');check('blocked-clean-stdout',b.returncode!=0 and not b.stdout)
        check('explicit-direct',call(root,'direct').returncode==0)
        b=call(root,'bridge');check('no-session-clean-stdout',b.returncode==3 and not b.stdout)
        check('restore-blocked',call(root,'block').returncode==0)
        # Default ports, IPv6, credentials and local-setting readback.
        for scheme,ports in [('http',[80,8080]),('https',[443,8443]),('socks5',[1080,1595]),('socks5h',[1080,1595])]:
            for host in ['127.0.0.1','[::1]','proxy.example.invalid']:
                for port in ports:
                    label=f'{scheme}-{host}-{port}'
                    url=f'{scheme}://fixture-user:fixture-pass@{host}:{port}'
                    result=call(root,'local-settings','set',data=json.dumps({'url':url}))
                    check(label+'-write',result.returncode==0)
                    public=json.loads(call(root,'local-settings').stdout)['url']
                    check(label+'-no-credentials','fixture-user' not in public and 'fixture-pass' not in public)
                    check(label+'-port',f':{port}' in public)
                    check(label+'-roundtrip',call(root,'local-settings','set',data=json.dumps({'url':public})).returncode==0)
        check('settings-do-not-select',json.loads(call(root,'status').stdout)['mode']=='blocked')
        # Import is all-or-nothing and never selects a route.
        text=work/'proxies.txt';text.write_text('\ufeff# test\nhttp://proxy.example.invalid:80:u:p:First\nhttps://proxy.example.invalid:443:u:p:Second\n',encoding='utf-8')
        check('import-valid',call(root,'import',str(text)).returncode==0)
        before=(root/'config/connections.json').read_bytes()
        check('duplicate-import',call(root,'import',str(text)).returncode==0)
        check('duplicate-preservation',(root/'config/connections.json').read_bytes()==before)
        text.write_text('http://proxy.example.invalid:8080:u:p:Good\nINVALID\n',encoding='utf-8')
        check('atomic-bad-import',call(root,'import',str(text)).returncode!=0 and (root/'config/connections.json').read_bytes()==before)
        text.write_bytes(b'#'+b'x'*(1024*1024))
        check('oversized-import',call(root,'import',str(text)).returncode!=0 and (root/'config/connections.json').read_bytes()==before)
        check('directory-import',call(root,'import',str(work)).returncode!=0)
        if hasattr(os,'mkfifo'):
            fifo=work/'fifo';os.mkfifo(fifo)
            check('fifo-no-writer-refused',call(root,'import',str(fifo)).returncode!=0)
        check('failed-select-preserves',call(root,'select','missing').returncode!=0 and (root/'config/connections.json').read_bytes()==before)
        # Registration respects ownership, malformed roots and symlink boundaries.
        for index,value in enumerate([[],False,42,'fixture',None,{'mcpServers':[]} ]):
            r=work/f'json-{index}';r.mkdir();(r/'.mcp.json').write_text(json.dumps(value),encoding='utf-8');original=(r/'.mcp.json').read_bytes()
            result=call(r,'register')
            check(f'json-{index}-controlled',result.returncode not in (0,101))
            check(f'json-{index}-preserved',(r/'.mcp.json').read_bytes()==original)
        r=work/'foreign';r.mkdir();foreign=work/'another-root';foreign.mkdir()
        (r/'.mcp.json').write_text(json.dumps({'mcpServers':{'Upwork-Agent-mcp':{'command':str(binary),'args':['--root',str(foreign),'bridge']},'other':{'command':'preserved'}}}),encoding='utf-8')
        original=(r/'.mcp.json').read_bytes();check('foreign-registration-refused',call(r,'register').returncode!=0 and (r/'.mcp.json').read_bytes()==original)
        r=work/'owned';r.mkdir();(r/'.mcp.json').write_text(json.dumps({'mcpServers':{'other':{'command':'preserved'}}}),encoding='utf-8')
        check('registration-first',call(r,'register').returncode==0)
        first=(r/'.mcp.json').read_bytes();settings=(r/'.claude/settings.local.json').read_bytes()
        check('registration-idempotent',call(r,'register').returncode==0 and (r/'.mcp.json').read_bytes()==first and (r/'.claude/settings.local.json').read_bytes()==settings)
        check('other-registration-preserved',json.loads(first)['mcpServers']['other']['command']=='preserved')
        if os.name=='posix':
            r=work/'linked';r.mkdir();outside=work/'outside';outside.mkdir();(r/'.claude').symlink_to(outside,target_is_directory=True)
            check('linked-registration-refused',call(r,'register').returncode!=0 and not list(outside.iterdir()) and not (r/'.mcp.json').exists())
        for p in work.rglob('*.log'):
            data=p.read_text(encoding='utf-8')
            check('credential-log-'+str(p.relative_to(work)),'fixture-user' not in data and 'fixture-pass' not in data)
    passed=sum(r['passed'] for r in receipts)
    output={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':len(receipts),'passed':passed,'failed':len(receipts)-passed,'child_processes':processes,'results':receipts}
    args.output.write_text(json.dumps(output,indent=2)+'\n',encoding='utf-8')
    LOG.info('Completed %d assertions across %d bounded child processes; %d failed.',len(receipts),processes,len(receipts)-passed)
    LOG.warning('Offline Linux CLI checks do not substitute for Windows/macOS or live account checks.')
    if passed!=len(receipts):raise SystemExit(1)
if __name__=='__main__':
    try:main()
    except Exception:
        LOG.error('CLI probe aborted; remaining checks are not marked passed.');raise
