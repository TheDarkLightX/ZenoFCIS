// Property tests of the existing integer input path: exact round trips, and
// refusal of every non-canonical form before a value can reach the core.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {describe,display,inDomain} from '../public/zenoui/app-contract.js';
import {initial,inputFor,step} from '../public/zenoui/app-state.js';

// A fixed-seed generator (SplitMix64 over BigInt): the same cases on every run.
function seeded(seed) {
  let state=BigInt(seed);const mask=(1n<<64n)-1n;
  return ()=>{
    state=(state+0x9e3779b97f4a7c15n)&mask;let z=state;
    z=((z^(z>>30n))*0xbf58476d1ce4e5b9n)&mask;z=((z^(z>>27n))*0x94d049bb133111ebn)&mask;
    return z^(z>>31n);
  };
}
const I128_MIN=-(1n<<127n),I128_MAX=(1n<<127n)-1n;
const field=(min,max)=>({path:'command.101.121',source:'command',index:1,domain:{kind:'int',min:String(min),max:String(max)},options:[],unit:'units'});
const DOMAINS=[field(0n,3n),field(-5n,5n),field(1n,1000000n),field(0n,(1n<<64n)-1n),field(-(1n<<53n)-7n,(1n<<53n)+7n),field(I128_MIN,I128_MAX)];
// A uniform-enough value in [min, max], biased to bounds and Number's precision edge.
function pick(next,{domain:{min,max}}) {
  const low=BigInt(min),high=BigInt(max),span=high-low+1n,r=next();
  const edges=[low,high,low+1n,high-1n,0n,-1n,1n,(1n<<53n)+1n,-(1n<<53n)-1n].filter(v=>v>=low&&v<=high);
  if(r%4n===0n)return edges[Number(r%BigInt(edges.length))];
  return low+((r<<64n)|next())%span;
}

test('every_value_in_a_domain_round_trips_exactly',()=>{
  const next=seeded(20261021);
  for(const f of DOMAINS)for(let i=0;i<2000;i++){
    const value=pick(next,f),text=value.toString();
    assert.ok(inDomain(f,text),`in-domain value refused: ${text} in ${f.domain.min}..${f.domain.max}`);
    assert.equal(BigInt(text).toString(),text,'value changed on its way back');
    assert.equal(display(f,text),`${text} units`,'display changed the digits');
  }
  // Above 2^53 a Number conversion would round; the path keeps every digit.
  for(const text of ['9007199254740993','-9007199254740993','170141183460469231731687303715884105727','-170141183460469231731687303715884105728'])
    assert.ok(inDomain(field(I128_MIN,I128_MAX),text),text);
});

test('values_beyond_the_domain_and_its_signs_are_refused',()=>{
  const next=seeded(7);
  for(const f of DOMAINS){
    const min=BigInt(f.domain.min),max=BigInt(f.domain.max);
    for(const beyond of [min-1n,max+1n,min-(1n<<70n),max+(1n<<70n),min-(next()%1000n)-1n,max+(next()%1000n)+1n])
      assert.equal(inDomain(f,beyond.toString()),false,`outside value accepted: ${beyond}`);
  }
  assert.equal(inDomain(field(0n,3n),'-1'),false,'a sign the domain does not allow');
  assert.equal(inDomain(field(0n,3n),'-0'),false,'negative zero');
});

test('non_canonical_forms_are_refused_in_every_domain',()=>{
  const forms=[
    // Exponents and other bases.
    '1e0','1E0','1e+0','1e-0','0x1','0X1','0o1','0b1','Infinity','NaN','1n',
    // Signs.
    '+1','+0','--1','-','- 1','−1','−'+'1','＋1','﹣1',
    // Whitespace, inside or around.
    ' 1','1 ','\t1','1\n',' 1','1 ','1 ','　1','1 0',
    // Locale separators and decimal points.
    '1,000','1.000','1\'000','1_000','1 000','1 000','1.0','1,0','.1','1.',
    // Other digit systems.
    '١','۱','१','１','\u{1d7cf}',
    // Leading zeros, empty and invisible characters.
    '01','00','-01','','​1','1​','‮1','﻿1',
  ];
  for(const f of DOMAINS)for(const text of forms)assert.equal(inDomain(f,text),false,`non-canonical form accepted: ${JSON.stringify(text)}`);
  // A 41-character value is refused even in a domain wide enough for its number.
  const wide=field(-(10n**60n),10n**60n);
  for(const text of ['1'.repeat(41),'-'+'1'.repeat(40),'9'.repeat(41)])assert.equal(inDomain(wide,text),false,`41 characters accepted: ${text}`);
  assert.ok(inDomain(wide,'-'+'1'.repeat(39)),'40 characters is the bound');
  for(const value of [1,1n,null,undefined,['1'],{}])assert.equal(inDomain(field(0n,3n),value),false,`non-string accepted: ${String(value)}`);
});

test('a_typed_value_reaches_the_core_only_in_canonical_form',()=>{
  const raw={fields:[
    {path:'pre.100.110',source:'state',index:0,domain:{kind:'int',min:'0',max:'9'},options:[],initial:'0'},
    {path:'command.101.120',source:'command',index:1,domain:{kind:'sum',type_id:101,variants:[120]},options:[{value:'120'}],initial:'120'},
    {path:'command.101.121',source:'command',index:2,domain:{kind:'int',min:'1',max:'1000000'},options:[],initial:'1'}],
    reasons:[],enablement:{table:[false,false,false,true]},domain:'1'};
  const meta={format:'zenoui/metadata/1',title:'Amounts',action:'command.101.120',
    fields:{'pre.100.110':{label:'Count',unit:'items'},'command.101.120':{label:'Command',unit:'unitless'},'command.101.121':{label:'Amount',unit:'items'}},
    variants:{'120':'Add'},commands:{'120':{label:'Add',consequential:true}},reasons:{}};
  const app=describe(raw,meta),start=initial(app,'a'.repeat(64));
  for(const text of ['1e3','+5','05',' 5','5 ','1,000','1.000','٥','1'.repeat(41),'0','1000001',''])
    assert.deepEqual(step(start,{type:'Edit',command:'120',index:2,value:text}).intents,[],`typed ${JSON.stringify(text)} reached the core`);
  for(const text of ['1','5','999999','1000000']){
    const {state,intents}=step(start,{type:'Edit',command:'120',index:2,value:text});
    assert.equal(intents.length,1);assert.deepEqual(intents[0].binding.input,inputFor(state,'120'));
    assert.equal(intents[0].binding.input[2],text,'the core saw other digits than were typed');
  }
});
