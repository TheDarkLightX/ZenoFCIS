#!/usr/bin/env python3
"""Regenerate service pins through the actual contract compiler (offline)."""
import argparse,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',required=True);p.add_argument('directory',type=Path);a=p.parse_args()
root=a.directory
pins=json.loads(subprocess.run([a.binary,'pins',str(root)],capture_output=True,text=True,check=True).stdout)
m=json.loads((root/'composition.json').read_text())
for core,pin in zip(m['components'],pins,strict=True):core.update({k:v for k,v in pin.items() if k!='schema_sha256'})
(root/'composition.json').write_text(json.dumps(m,indent=2)+'\n')
read_law='accepted && (request.op == 5 || request.op == 6) -> choose(request.op == 6, pre.keydir.device_2, pre.keydir.device_1) == 144 && choose(request.target == 1, pre.mailbox.slot_1_generation, pre.mailbox.slot_2_generation) <= choose(request.op == 6, pre.keydir.device_2_generation, pre.keydir.device_1_generation) && choose(request.target == 1, post.mailbox.slot_1_status, post.mailbox.slot_2_status) == 142'
service={'schema':'zeno-fcis/service/1','composition':m,'schemas':[x['schema_sha256'] for x in pins],
         'laws':[{'name':'read-selected-device','formula':read_law,'genesis':False}], 'max_steps':1000000}
(root/'service.json').write_text(json.dumps(service,indent=2)+'\n')
