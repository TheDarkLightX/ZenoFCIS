"""Actual baseline/candidate catalog and full export/replay measurements."""
from pathlib import Path
import argparse,builtins,copy,hashlib,json,os,resource,statistics,sys,time

HERE=Path(__file__).resolve().parent
parser=argparse.ArgumentParser();parser.add_argument('--root',required=True);parser.add_argument('--case',required=True);parser.add_argument('--catalog-only',action='store_true');parser.add_argument('--bounded',action='store_true');args=parser.parse_args()
if args.bounded:
    resource.setrlimit(resource.RLIMIT_CPU,(90,90));resource.setrlimit(resource.RLIMIT_AS,(1024**3,1024**3))
    os.sched_setaffinity(0,{min(os.sched_getaffinity(0))})
ROOT=Path(args.root);sys.path.insert(0,str(ROOT/'integrations/zal'))
from zal_notation import core,lens_library as V,lenses as L,library_lens_browser as B
def need(value,message):
    if not value:raise RuntimeError(message)
def sha(data):return hashlib.sha256(data).hexdigest()
def encoded(value):return (core.canonical(value)+'\n').encode()
output=HERE/'measures'/args.case;output.mkdir(parents=True,exist_ok=False)
record={'case':args.case,'producer_sha256':sha((ROOT/'integrations/zal/zal_notation/lens_library.py').read_bytes()),
        'shared_implementation':L.IMPLEMENTATION,'pass':False,'workloads':{}}
start=time.perf_counter();cpu=time.process_time()

def catalog_counts():
    counts={'validate_definition':0,'validate_registry':0,'register':0}
    codes={getattr(V,n).__code__:n for n in counts}
    def trace(frame,event,arg):
        if event=='call' and frame.f_code in codes:counts[codes[frame.f_code]]+=1
    sys.setprofile(trace)
    try:value=V.default_registry()
    finally:sys.setprofile(None)
    return value,counts

def public_results(catalog):
    bad=[]
    for name,edit in [('duplicate',lambda x:x['definitions'].append(copy.deepcopy(x['definitions'][0]))),
                      ('extra-and-wrong-schema',lambda x:x.update(extra=1,schema='wrong')),
                      ('wrong-schema',lambda x:x.update(schema='wrong')),
                      ('boolean-id',lambda x:x['definitions'][0].update(id=True)),
                      ('extra-definition-field',lambda x:x['definitions'][0].update(executable='not executed')),
                      ('bad-mode',lambda x:x['definitions'][0].update(mode='execute')),
                      ('bad-list-type',lambda x:x['definitions'][0].update(assumptions='wrong')),
                      ('too-many',lambda x:x.update(definitions=[copy.deepcopy(x['definitions'][0]) for _ in range(65)]))]:
        value=copy.deepcopy(catalog);edit(value);before=copy.deepcopy(value)
        try:V.validate_registry(value);answer={'status':'accepted'}
        except Exception as e:answer={'status':'refused','type':type(e).__name__,'detail':str(e)}
        need(value==before,'Validator mutated caller data');bad.append({'case':name,**answer})
    base=V.new_registry();proposed=copy.deepcopy(catalog['definitions'][0]);registered=V.register(base,proposed)
    need(base==V.new_registry(),'Registration mutated supplied registry')
    proposed['assumptions'].append('caller edit');need(proposed!=registered['definitions'][0],'Registration shared caller fields')
    admission=[]
    for definition in catalog['definitions']:
        altered=copy.deepcopy(definition);altered['summary']+=' Caller annotation.'
        admission.append({'id':definition['id'],'exact':V._adapter(definition),'altered':V._adapter(altered)})
    return {'refusals':bad,'admission':admission}

def semantic_packet(packet):
    """Declared source-identity projection; source/model/law/evidence data kept."""
    value=copy.deepcopy(packet);value.pop('producerDigest');value.pop('producerSourcePins')
    for subject in value['subjects']:
        for view in subject['lenses']:
            view.pop('lensRevision',None)
            pair=view.get('_receipt')
            if pair is None:continue
            app=pair['application'];need(app['schema']=='zal.library-lens-application/1','Unexpected application schema')
            app.pop('implementation_digest')
            payload=app['payload']
            if payload.get('schema')=='zal.lens-application/1':
                payload.pop('implementation_digest');payload.pop('lens_revision')
            receipt=pair['receipt'];need(receipt['schema']=='zal.library-lens-check/1','Unexpected receipt schema')
            receipt.pop('application_digest',None)
            evidence=receipt.get('evidence',{})
            if evidence.get('schema')=='zal.lens-check/1':
                evidence.pop('implementation_digest');evidence.pop('application_digest',None)
    return value

try:
    catalog,counts=catalog_counts();canonical=core.canonical(catalog)
    (output/'catalog.json').write_bytes(encoded(catalog));record['definitions']=len(catalog['definitions']);record['validation_calls']=counts
    mutated=V.default_registry()
    for row in mutated['definitions']:
        for field in ['assumptions','preserves','forgets','adds','profile_hints']:row[field].append('Caller edit')
    need(core.canonical(V.default_registry())==canonical,'Fresh catalog ownership failed');record['fresh_ownership']=True
    fallback={};original_import=builtins.__import__
    for exception in [ImportError,ValueError]:
        def hook(name,globals=None,locals=None,fromlist=(),level=0):
            if level==1 and 'lenses' in (fromlist or ()):raise exception('Fixed optional adapter unavailable')
            return original_import(name,globals,locals,fromlist,level)
        builtins.__import__=hook
        try:fallback[exception.__name__]=V.default_registry()
        finally:builtins.__import__=original_import
    record['fallback_sha256']=sha(encoded(fallback));(output/'fallback.json').write_bytes(encoded(fallback))
    result=public_results(catalog);(output/'public-results.json').write_bytes(encoded(result));record['public_results_sha256']=sha(encoded(result))
    samples=[]
    for _ in range(25):
        begun=time.perf_counter_ns();value=V.default_registry();samples.append(time.perf_counter_ns()-begun)
        need(value==catalog,'Timed catalog changed')
    record['catalog_times_ns']=samples;record['catalog_median_ns']=statistics.median(samples)
    record['catalog_encoded_bytes']=len(encoded(catalog));record['catalog_sha256']=sha(encoded(catalog))
    for name in ([] if args.catalog_only else ['representative-five','heldout-latch']):
        request=json.loads((HERE/'workloads'/(name+'.json')).read_text());dest=output/name
        begun=time.perf_counter();export=B.export(request,dest);export_seconds=time.perf_counter()-begun
        begun=time.perf_counter();replay=B.replay(dest,export['request_sha256']);replay_seconds=time.perf_counter()-begun
        packet=json.loads((dest/'data/packet.json').read_text());projection=semantic_packet(packet)
        (output/(name+'-semantic.json')).write_bytes(encoded(projection))
        totals=sum(p.stat().st_size for p in dest.rglob('*') if p.is_file())
        record['workloads'][name]={'export_seconds':export_seconds,'replay_seconds':replay_seconds,
            'total_seconds':export_seconds+replay_seconds,'artifact_bytes':totals,'files':export['files'],
            'request_sha256':export['request_sha256'],'semantic_projection_sha256':sha(encoded(projection)),
            'replay_status':replay['status'],'producer_source_pins':packet['producerSourcePins'],
            'view_statuses':[(s['object']['id'],v['id'],v['status']) for s in packet['subjects'] for v in s['lenses']]}
        print(json.dumps({'case':args.case,'workload':name,'export_seconds':export_seconds,'replay_seconds':replay_seconds,'files':export['files']}),flush=True)
    record['pass']=True
except BaseException as e:
    record['failure']={'type':type(e).__name__,'message':str(e)}
finally:
    record.update(wall_seconds=time.perf_counter()-start,cpu_seconds=time.process_time()-cpu,
                  peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
    (output/'RESULT.json').write_text(json.dumps(record,indent=2,sort_keys=True)+'\n')
    print(json.dumps({k:v for k,v in record.items() if k not in ['workloads','catalog_times_ns']}),flush=True)
if not record['pass']:raise SystemExit(1)
