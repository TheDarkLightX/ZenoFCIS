// A confirmed request is sent only when the server's fresh preview decides the
// same inputs; a preview for other inputs never reaches /command.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {describe} from '../public/zenoui/app-contract.js';
import {DurableTransport,pendingKey} from '../public/zenoui/durable.js';

const raw={fields:[
  {path:'pre.100.110',source:'state',index:0,domain:{kind:'int',min:'0',max:'9'},options:[],initial:'0'},
  {path:'command.101.120',source:'command',index:1,domain:{kind:'sum',type_id:101,variants:[120]},options:[{value:'120'}],initial:'120'},
  {path:'command.101.121',source:'command',index:2,domain:{kind:'int',min:'1',max:'9'},options:[],initial:'1'}],
  reasons:[],enablement:{table:[false,false,false,true]},domain:'90'};
const meta={format:'zenoui/metadata/1',title:'Amounts',action:'command.101.120',
  fields:{'pre.100.110':{label:'Count',unit:'items'},'command.101.120':{label:'Command',unit:'unitless'},'command.101.121':{label:'Amount',unit:'items'}},
  variants:{'120':'Add'},commands:{'120':{label:'Add',consequential:true}},reasons:{}};
const bundle={description:describe(raw,meta)};
const session={contract:'a'.repeat(64),principal:'b'.repeat(64)};
const binding={...session,revision:'0',input:['0','120','2']};

async function dispatch(decided) {
  const requests=[],storage=new Map();
  storage.getItem=k=>storage.get(k)??null;storage.setItem=(k,v)=>storage.set(k,v);storage.removeItem=k=>storage.delete(k);
  const fetcher=async(path)=>{
    requests.push(path);
    if(path==='/preview')return {ok:true,json:async()=>({reply:{input:decided,class:'Accept',reason:null,post:['2'],checked:'1',total:'1'},command:'00'})};
    return {ok:false,status:503};
  };
  const transport=new DurableTransport(bundle,'c'.repeat(64),session,{fetcher,storage,random:bytes=>bytes.fill(7)});
  const message=new Promise(resolve=>{transport.onmessage=({data})=>resolve(data);});
  transport.postMessage({kind:'dispatch',binding});
  const data=await message;
  return {requests,data,pending:storage.get(pendingKey)};
}

test('a_preview_of_other_inputs_is_never_dispatched',async()=>{
  for(const decided of [['0','120','3'],['1','120','2'],['0','120','2','0']]){
    const {requests,data,pending}=await dispatch(decided);
    assert.equal(data.kind,'error');
    assert.deepEqual(requests,['/preview'],`a changed preview was dispatched: ${decided}`);
    assert.equal(pending,undefined,'a changed request was saved for recovery');
  }
});
test('a_preview_of_the_confirmed_inputs_is_dispatched',async()=>{
  const {requests}=await dispatch(['0','120','2']);
  assert.deepEqual(requests,['/preview','/command'],'the confirmed request was not sent');
});
