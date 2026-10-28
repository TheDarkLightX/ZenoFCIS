import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {describe,checkPresentation,verifyPresentation} from '../public/zenoui/app-contract.js';
import {initial,step,view} from '../public/zenoui/app-state.js';
import {presentationView} from '../public/zenoui/ui-view.js';
import {editFixture,approvalFixture} from './idiom-fixtures.mjs';

const metadata=JSON.parse(await readFile(new URL('../../examples/contract-tables/library-limits/ui.json',import.meta.url)));
// Diagnostic descriptor shape only. This is NOT a native capture or evidence
// that the Rust source compiles. The separate native test requires real output.
function diagnosticDescriptor(){
 const f=(field,type,index,path,source,domain,initial,options=[])=>({field,type,index,path,source,domain,initial,options,type_name:`type_${type}`});
 const fields=Array.from({length:6},(_,i)=>f(110+i,105,i,`pre.100.${110+i}`,'state',{kind:'int',min:'0',max:'3'},'0'));
 fields.push(f(160,109,6,'pre.100.160','state',{kind:'int',min:'1',max:'2'},'2'),
  f(120,108,7,'command.101.120','command',{kind:'sum',type_id:108,variants:[150,151]},'150',[{value:'150'},{value:'151'}]),
  f(121,106,8,'command.101.121','command',{kind:'int',min:'0',max:'5'},'0'),
  f(122,107,9,'command.101.122','command',{kind:'int',min:'1',max:'3'},'1'),
  f(null,102,10,'context.102','context',{kind:'sum',type_id:102,variants:[130,131]},'130',[{value:'130'},{value:'131'}]));
 return {fields,reasons:[200,201,202,203].map(id=>({id:String(id)})),domain:'589824',enablement:{table:[false,false,false,true]},
  nativeTables:{format:'zenoui/native-fixed-tables/1',tables:[{id:'110',owner:'100',name:'copies',fieldType:'105',size:6,
   entries:Array.from({length:6},(_,i)=>({field:String(110+i),path:`pre.100.${110+i}`,index:i}))}]}};
}
function walk(node,fn,path='$'){fn(node,path);(node.children??[]).forEach((child,i)=>walk(child,fn,`${path}.${i}`));}

test('bound native coordinates select the existing table-list with single state coverage',()=>{
 const app=describe(diagnosticDescriptor(),structuredClone(metadata));
 assert.equal(app.format,'zenoui/app-description/3');
 assert.deepEqual(app.tables,[{id:'110',label:'Book copies',entries:Array.from({length:6},(_,i)=>`pre.100.${110+i}`)}]);
 const counts=new Map(),tables=[];
 walk(app.presentation.ast,node=>{if(node.kind==='display'){if(node.idiom==='table-list')tables.push(node);for(const p of node.fields)counts.set(p,(counts.get(p)??0)+1);}});
 assert.equal(tables.length,1);
 assert.deepEqual(tables[0].fields,app.tables[0].entries);
 for(const field of app.fields.filter(f=>f.source==='state'))assert.equal(counts.get(field.path),1);
 assert.equal(checkPresentation(app),true);assert.equal(verifyPresentation(app),true);
});

test('missing, stale, foreign, reordered or coercible table coordinates refuse',()=>{
 const changes=[r=>delete r.nativeTables,r=>r.nativeTables.format='unknown',r=>r.nativeTables.tables[0].owner='101',
  r=>r.nativeTables.tables[0].fieldType='107',r=>r.nativeTables.tables[0].entries.reverse(),
  r=>r.nativeTables.tables[0].entries[0].index=7,r=>r.nativeTables.tables[0].entries[0].path='command.101.120',
  r=>r.nativeTables.tables[0].entries[1].field='110',r=>r.nativeTables.tables[0].size=65,
  r=>r.fields[0].field='110',r=>r.fields[0].type='105',r=>r.fields[0].source='command'];
 for(const change of changes){const raw=diagnosticDescriptor();change(raw);assert.throws(()=>describe(raw,structuredClone(metadata)),/Ui/);}
 for(const change of [m=>delete m.tables['110'],m=>m.tables['999']={label:'Foreign'},m=>m.tables['110'].unit='credits',
  m=>m.fields['pre.100.111'].unit='credits',m=>m.fields['pre.100.111'].decimals=1]){
  const meta=structuredClone(metadata);change(meta);assert.throws(()=>describe(diagnosticDescriptor(),meta),/Ui/);
 }
});

test('forged description grouping and unbound legacy table claims still refuse at mount checks',()=>{
 const app=describe(diagnosticDescriptor(),structuredClone(metadata));
 for(const change of [a=>a.tables[0].entries.pop(),a=>a.tables[0].entries.reverse(),a=>a.tables.push(structuredClone(a.tables[0])),
  a=>a.nativeTables.tables[0].entries[0].index=1,a=>a.format='zenoui/app-description/2']){
  const forged=structuredClone(app);change(forged);assert.throws(()=>checkPresentation(forged),/Ui/);
 }
 for(const extra of [{tables:[]},{semantics:{}}])assert.throws(()=>describe({...diagnosticDescriptor(),...extra},structuredClone(metadata)),/UiUnsupportedUnboundSemanticsOrTables/);
 const {raw,meta}=editFixture(),scalar=structuredClone(describe(raw,meta));scalar.tables=[];
 assert.throws(()=>checkPresentation(scalar),/UiUnsupportedUnboundSemanticsOrTables/);
});

test('masked table cells stay absent while visible cells retain exact units',()=>{
 const app=describe(diagnosticDescriptor(),structuredClone(metadata));
 let state=initial(app,'a'.repeat(64));
 state=step(state,{type:'Snapshot',values:['2',null,'0','0','0','0','2'],revision:'1'}).state;
 const data=view(state),rendered=presentationView(app,data,{server:true});let tablePath;
 walk(app.presentation.ast,(node,path)=>{if(node.kind==='display'&&node.idiom==='table-list')tablePath=path;});
 const table=rendered.nodes[tablePath];
 assert.deepEqual(table.entries.map(e=>e.index),[0,2,3,4,5]);
 assert.equal(table.entries[0].value,'2 member');
 assert.equal(data.state.some(f=>f.index===1),false);
 assert.equal(checkPresentation(app),true);
});

test('old scalar metadata keeps its version and unbound table refusal',()=>{
 for(const make of [editFixture,approvalFixture]){
  const {raw,meta}=make(),app=describe(raw,meta);assert.equal(app.format,'zenoui/app-description/2');assert.equal(Object.hasOwn(app,'tables'),false);
  assert.throws(()=>describe({...raw,tables:[]},meta),/UiUnsupportedUnboundSemanticsOrTables/);
 }
 const raw=diagnosticDescriptor(),meta=structuredClone(metadata);meta.format='zenoui/metadata/1';delete meta.tables;delete meta.requirements;
 const now=describe(raw,meta);
 assert.equal(Object.hasOwn(now,'tables'),false);assert.equal(now.format,'zenoui/app-description/2');
});
