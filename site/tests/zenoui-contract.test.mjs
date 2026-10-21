import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {canonical,describe,display} from '../public/zenoui/app-contract.js';
// The earlier canonical encoder is an independent differential oracle for JSON data.
function previous(value) {
 return JSON.stringify(Array.isArray(value)?value.map(v=>JSON.parse(previous(v))):value&&typeof value==='object'
   ?Object.fromEntries(Object.keys(value).sort().map(k=>[k,JSON.parse(previous(value[k]))])):value);
}
test('canonical_JSON_bytes_are_preserved_by_single_pass_sorting',()=>{
 const values=[null,true,false,0,-1,'9007199254740993','quotes " and \n and \u2603',[],{}];
 for(let i=0;i<100;i++)values.push({z:[String(i),{unit:'credits',value:'9007199254740993'}],a:{revision:String(i),principal:'Local operator',nested:[null,false,{}]}});
 for(const value of values){assert.equal(canonical(value),previous(value));if(value&&typeof value==='object'&&!Array.isArray(value))assert.equal(canonical(value),canonical(Object.fromEntries(Object.entries(value).reverse())));}
});
test('metadata_refuses_app_specific_code_and_raw_HTML',async()=>{
 const dir=new URL('../zenoui/test-projects/wide-counter/ui.json',import.meta.url);
 const meta=JSON.parse(await readFile(dir));
 // These fields are not in the closed metadata schema, regardless of contract contents.
 for(const field of ['script','css','html','events','claims'])assert.throws(()=>describe({}, {...meta,[field]:'anything'}),/UiInvalidMetadata/);
 assert.throws(()=>describe({fields:[{path:'x'}]}, {...meta,title:'<script>bad</script>'}),/UiMissingLabel:title/);
});
test('integer_value_labels_are_optional_checked_and_displayed',()=>{
 const raw={fields:[
  {path:'pre.100.110',source:'state',index:0,domain:{kind:'int',min:'0',max:'3'},options:[],initial:'0'},
  {path:'command.101',source:'command',index:1,domain:{kind:'sum',type_id:101,variants:[120]},options:[{value:'120'}],initial:'120'}],
  reasons:[],enablement:{table:[false,false,false,true]},domain:'4'};
 const meta=values=>({format:'zenoui/metadata/1',title:'Shelf',action:'command.101',
  fields:{'pre.100.110':values===undefined?{label:'Holder',unit:'member'}:{label:'Holder',unit:'member',values},'command.101':{label:'Command',unit:'unitless'}},
  variants:{'120':'Borrow'},commands:{'120':{label:'Borrow',consequential:true}},reasons:{}});
 const plain=describe(raw,meta());
 assert.equal(Object.hasOwn(plain.fields[0],'values'),false,'unlabelled fields keep their description');
 assert.equal(display(plain.fields[0],'0'),'0 member');
 assert.equal(display(plain.fields[1],'120'),'Borrow','unitless is not shown');
 const labelled=describe(raw,meta({'0':'on shelf'}));
 assert.equal(display(labelled.fields[0],'0'),'on shelf');
 assert.equal(display(labelled.fields[0],'2'),'2 member');
 for(const bad of [{},{'9':'nowhere'},{'00':'shelf'},{'-1':'shelf'},['shelf'],{'0':''},{'0':'<b>'}])
  assert.throws(()=>describe(raw,meta(bad)),/UiInvalidValueLabels|UiMissingLabel/);
 const onSum=meta();onSum.fields['command.101']={label:'Command',unit:'unitless',values:{'120':'x'}};
 assert.throws(()=>describe(raw,onSum),/UiInvalidValueLabels/);
 const extra=meta();extra.fields['pre.100.110']={label:'Holder',unit:'member',names:{}};
 assert.throws(()=>describe(raw,extra),/UiInvalidFieldMetadata/);
});
test('contract_guide_documents_exactly_the_metadata_keys',async()=>{
 const guide=await readFile(new URL('../../docs/CONTRACT_RULES.md',import.meta.url),'utf8');
 const tables=guide.split('## UI metadata: `ui.json`')[1].split('## Adoptions')[0].split(/\n#+ /);
 const keys=table=>table.split('\n').filter(l=>/^\| `/.test(l)).map(l=>l.split('|')[1].trim().replaceAll('`',''));
 const meta=JSON.parse(await readFile(new URL('../zenoui/test-projects/wide-counter/ui.json',import.meta.url)));
 assert.deepEqual(keys(tables[0]).sort(),Object.keys(meta).sort());
 for(const key of keys(tables[0])){const bad={...meta};delete bad[key];assert.throws(()=>describe({},bad),/UiInvalidMetadata/);}
 assert.deepEqual(keys(tables[1]),['label','unit','values']);
 assert.deepEqual(keys(tables[2]),['label','consequential']);
});
test('a_server_clock_field_is_checked_and_left_out_of_reply_matching',async()=>{
 const {validReply}=await import('../public/zenoui/app-state.js');
 const raw=clock=>({fields:[
  {path:'pre.100.110',source:'state',index:0,domain:{kind:'int',min:'0',max:'3'},options:[],initial:'0'},
  {path:'command.101',source:'command',index:1,domain:{kind:'sum',type_id:101,variants:[120]},options:[{value:'120'}],initial:'120'},
  {path:'context.102.131',source:'context',index:2,domain:{kind:'int',min:'0',max:'14'},options:[],initial:'0',...clock}],
  reasons:[],enablement:{table:[false,false,false,true]},domain:'60'});
 const meta={format:'zenoui/metadata/1',title:'Deadline',action:'command.101',
  fields:{'pre.100.110':{label:'Count',unit:'items'},'command.101':{label:'Command',unit:'unitless'},'context.102.131':{label:'Day',unit:'unitless'}},
  variants:{'120':'Add'},commands:{'120':{label:'Add',consequential:true}},reasons:{}};
 const clocked=describe(raw({clock:true}),meta),plain=describe(raw({}),meta);
 const reply=day=>({input:['0','120',day],class:'Accept',reason:null,post:['1'],checked:'1',total:'1'});
 assert.ok(validReply(clocked,['0','120','3'],reply('5')),'the server sets the clock');
 assert.equal(validReply(plain,['0','120','3'],reply('5')),false,'other context values must match');
 assert.equal(validReply(clocked,['1','120','3'],reply('5')),false,'other inputs must match');
 for(const bad of [{clock:false},{clock:'yes'}])assert.throws(()=>describe(raw(bad),meta),/UiInvalidClock/);
 const onState=raw({});onState.fields[0].clock=true;
 assert.throws(()=>describe(onState,meta),/UiInvalidClock/);
});
