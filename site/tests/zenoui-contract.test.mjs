import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {canonical,describe} from '../public/zenoui/app-contract.js';
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
