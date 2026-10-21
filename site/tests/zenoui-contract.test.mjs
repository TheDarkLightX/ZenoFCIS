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
