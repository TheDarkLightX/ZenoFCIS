"""Pinned matched registry measurements, sequential workers and owned cleanup."""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,sys,time
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
from owned_process import Owned,wait_owned
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--pins-sha256',required=True);parser.add_argument('--check-only',action='store_true');args=parser.parse_args()
    if sha(HERE/'MEASURE_PINS_01.json')!=args.pins_sha256:raise RuntimeError('Reviewed pins changed')
    pins=json.loads((HERE/'MEASURE_PINS_01.json').read_text())
    def stable():
        for role in ['baseline','candidate']:
            item=pins[role]
            for name,h in item['files'].items():
                if sha(Path(item['root'])/name)!=h:raise RuntimeError(role+' source changed: '+name)
        for name,h in pins['helpers'].items():
            if sha(HERE/name)!=h:raise RuntimeError('Measurement helper changed: '+name)
        for name,h in pins['workloads'].items():
            if sha(HERE/'workloads'/name)!=h:raise RuntimeError('Workload changed: '+name)
        if sha(pins['python']['path'])!=pins['python']['sha256']:raise RuntimeError('Python changed')
    stable()
    if args.check_only:print(json.dumps({'status':'pins-verified','launch':'not-performed'}));return
    if Path('/proc/1/comm').read_text().strip()!='systemd':raise RuntimeError('Actual host required')
    result={'schema':'zal/registry-matched-measurements/1','pass':False,'pins_sha256':args.pins_sha256,'workers':[]}
    baseline_result=None;owner=None
    schedule=['baseline','candidate','candidate','baseline','baseline','candidate']
    try:
        for index,role in enumerate(schedule,1):
            stable();case=role+'-'+str(index).zfill(2);start=time.monotonic()
            command=[pins['python']['path'],'-I','-B',str(HERE/'worker.py'),'--bounded','--root',pins[role]['root'],'--case',case]
            env=os.environ.copy();env['PYTHONDONTWRITEBYTECODE']='1';env.pop('PYTHONOPTIMIZE',None)
            with (HERE/(case+'.log')).open('wb') as log:
                p=subprocess.Popen(command,cwd=HERE,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True);owner=Owned(p)
                try:code=wait_owned(p,owner,120)
                finally:cleanup=owner.finish();owner=None
            stable();path=HERE/'measures'/case/'RESULT.json';worker=json.loads(path.read_text())
            row={'case':case,'backend':role,'exit':code,'wall_with_startup_seconds':time.monotonic()-start,
                 'result_sha256':sha(path),'cleanup':cleanup}
            result['workers'].append(row)
            if code or not worker['pass'] or cleanup['survivors']:raise RuntimeError('Worker incomplete: '+case+' '+str(worker.get('failure')))
            if baseline_result is None:baseline_result=worker
            else:
                for key in ['catalog_sha256','fallback_sha256','public_results_sha256','catalog_encoded_bytes']:
                    if worker[key]!=baseline_result[key]:raise RuntimeError('Differential mismatch: '+key+' '+case)
                for name,data in worker['workloads'].items():
                    old=baseline_result['workloads'][name]
                    for key in ['semantic_projection_sha256','request_sha256','files','view_statuses','replay_status']:
                        if data[key]!=old[key]:raise RuntimeError('Semantic outcome mismatch: '+name+' '+key+' '+case)
                    oldpins=old['producer_source_pins'];newpins=data['producer_source_pins']
                    differences=[n for n in oldpins if oldpins[n]!=newpins.get(n)]
                    expected=[] if role=='baseline' else ['zal_notation/lens_library.py']
                    if differences!=expected or oldpins.keys()!=newpins.keys():raise RuntimeError('Unexpected producer identity difference')
            print(json.dumps({'completed':case,'seconds':row['wall_with_startup_seconds'],'peak_rss_kib':worker['peak_rss_kib']}),flush=True)
            (HERE/'MEASURE_PROGRESS.json').write_text(json.dumps(result,indent=2)+'\n')
        result['pass']=True;result['inputs_stable']=True
    except BaseException as error:result['failure']={'type':type(error).__name__,'message':str(error)}
    finally:
        if owner is not None:result['interrupted_cleanup']=owner.finish()
        (HERE/'MEASURE_RESULT.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='workers'}),flush=True)
    if not result['pass']:raise SystemExit(1)
if __name__=='__main__':main()
