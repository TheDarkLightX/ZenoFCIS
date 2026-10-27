// The interface's own label check refuses the same invisible characters as
// the build's ui.json check (tools/zenoui.py).
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {describe} from '../public/zenoui/app-contract.js';

const raw={fields:[
  {path:'pre.100.110',source:'state',index:0,domain:{kind:'int',min:'0',max:'3'},options:[],initial:'0'},
  {path:'command.101',source:'command',index:1,domain:{kind:'sum',type_id:101,variants:[120]},options:[{value:'120'}],initial:'120'}],
  reasons:[{id:'200'}],enablement:{table:[false,false,false,true]},domain:'4'};
const meta=()=>({format:'zenoui/metadata/1',title:'Shelf',action:'command.101',
  fields:{'pre.100.110':{label:'Holder',unit:'member',values:{'0':'on shelf'}},'command.101':{label:'Command',unit:'unitless'}},
  variants:{'120':'Borrow'},commands:{'120':{label:'Borrow',consequential:true}},reasons:{'200':'Only members borrow.'}});
const PLACES={
  title:(m,c)=>{m.title='Shelf'+c;},
  label:(m,c)=>{m.fields['pre.100.110'].label=c+'Holder';},
  unit:(m,c)=>{m.fields['pre.100.110'].unit='mem'+c+'ber';},
  'value label':(m,c)=>{m.fields['pre.100.110'].values['0']='on'+c+' shelf';},
  variant:(m,c)=>{m.variants['120']='Bor'+c+'row';},
  command:(m,c)=>{m.commands['120'].label='Borrow'+c;},
  reason:(m,c)=>{m.reasons['200']='Only '+c+'members borrow.';},
};
const REFUSED=['‮','‪','⁨','⁩','‏','؜','​','‍','⁠','⁣','﻿','­',
  '͏','ㅤ','️','\u{e0041}','￹','\u0007','\t','\n','\u007f','\u0085','\u009b',' ',' ','','\ud800'];

test('labels_refuse_invisible_characters_in_every_place',()=>{
  assert.ok(describe(raw,meta()),'plain metadata refused');
  for(const character of REFUSED)for(const [place,change] of Object.entries(PLACES)){
    const m=meta();change(m,character);
    assert.throws(()=>describe(raw,m),/^Error: Ui/,`invisible character accepted: U+${character.codePointAt(0).toString(16)} in ${place}`);
  }
});
test('labels_in_any_script_are_kept',()=>{
  const m=meta();m.title='Crédit — 信用 — Кредит — קרדיט — رصيد — 💳 €5';m.fields['pre.100.110'].values={'0':'épuisé','1':'一'};
  assert.ok(describe(raw,m));
});
