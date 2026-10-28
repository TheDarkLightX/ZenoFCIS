"""Source freshness and actual process-interruption controls for the public shell."""
import copy
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
import run

ROOT=Path(__file__).resolve().parent
def main():
    old=run.ROOT
    controls=[]
    with tempfile.TemporaryDirectory(prefix='zal-count-binding-') as temp:
        root=Path(temp);(root/'evidence').mkdir();(root/'.build').mkdir()
        shutil.copytree(ROOT/'src',root/'src');shutil.copytree(ROOT/'app',root/'app')
        shutil.copy2(ROOT/'evidence/build.json',root/'evidence/build.json')
        shutil.copy2(ROOT/'.build/zal-relational-worker',root/'.build/zal-relational-worker')
        def refused(label):
            try:run.binding()
            except ValueError as error:controls.append({'case':label,'refusal':str(error)})
            else:raise AssertionError('stale input admitted: '+label)
        run.ROOT=root
        try:
            run.binding()
            source=root/'src/Zal/Logic/Syntax.hs';original=source.read_bytes()
            source.write_bytes(original+b'\n-- deliberate drift\n');refused('source changed');source.write_bytes(original)
            extra=root/'src/Extra.hs';extra.write_text('module Extra where\n');refused('source inventory changed');extra.unlink()
            binary=root/'.build/zal-relational-worker'
            with binary.open('ab') as handle:handle.write(b'changed')
            refused('binary changed')
        finally:run.ROOT=old
    q=copy.deepcopy(json.loads((ROOT/'examples/even-graph-count.json').read_text()));q['domain_size']='1000'
    stopped=subprocess.run([sys.executable,str(ROOT/'run.py'),'-','--seconds','1'],input=run.canonical(q),
                           stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=5)
    answer=json.loads(stopped.stdout)
    if stopped.returncode!=2 or answer.get('status') not in ('ResourceExhausted','Incomplete') or 'value' in answer:
        raise AssertionError(('interrupted job claimed success',answer))
    controls.append({'case':'actual CPU/wall interruption','response':answer})
    record={'script_sha256':run.sha(Path(__file__).read_bytes()),'shell_sha256':run.sha((ROOT/'run.py').read_bytes()),
            'haskell_binary_sha256':run.binding()[1]['binary_sha256'],'controls':controls}
    (ROOT/'evidence/shell-controls.json').write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps({'controls':len(controls),'interrupted_status':answer['status']}))
if __name__=='__main__':main()
