// Pure presentation state. The application authority still decides domain requests.
export const ACTS = Object.freeze(['synthesis','inventory','search','money']);
export const EVENTS = Object.freeze(['Start','Progress','Result','Select','CellResult','Fail','Cancel','PreferencesChanged','DraftChanged']);
export const REGIONS = Object.freeze(['instrument','metrics','comparison','record']);
export const PHASES = Object.freeze(['idle','running','done','error']);
export const DESCRIPTORS = freeze({
  checked:{label:'Inputs checked',type:'integer',unit:'inputs'},total:{label:'Declared domain',type:'integer',unit:'inputs'},
  passed:{label:'Within every applicable law',type:'integer',unit:'inputs'},differences:{label:'Decisions changed',type:'integer',unit:'decisions'},
  law_checks:{label:'Law checks passed',type:'integer',unit:'checks'},elapsed:{label:'In this browser',type:'decimal',unit:'ms'},
  assignments:{label:'Orders tried',type:'integer',unit:'orders'},refuted:{label:'Refuted by an exact input',type:'integer',unit:'orders'},
  nodes:{label:'Nodes, before → after',type:'integer pair',unit:'nodes'},work:{label:'Search work',type:'integer',unit:'work units'},
  quantity:{label:'Quantity',type:'integer',unit:'stock units'},available:{label:'Available stock',type:'integer',unit:'stock units'},
  reserved:{label:'Reserved stock',type:'integer',unit:'stock units'},authorized:{label:'Demonstration operator flag',type:'boolean',unit:'unitless'},
  action:{label:'Action',type:'enum',unit:'unitless'},reason:{label:'Reason',type:'enum',unit:'unitless'},law:{label:'Applicable law',type:'enum',unit:'unitless'},
  fee:{label:'Fee, rounded up',type:'u128 decimal',unit:'input atoms'},fee_bps:{label:'Fee rate',type:'integer',unit:'basis points'},
  net:{label:'Net amount in',type:'u128 decimal',unit:'input atoms'},out:{label:'Amount out, rounded down',type:'u128 decimal',unit:'output atoms'},
  reserve_in:{label:'Input reserve after',type:'u128 decimal',unit:'input atoms'},reserve_out:{label:'Output reserve after',type:'u128 decimal',unit:'output atoms'},
  product:{label:'Product of reserves',type:'boolean',unit:'input atoms × output atoms'},steps:{label:'Instructions attempted',type:'integer',unit:'instructions'},
  boolean_input:{label:'Input, in declared order',type:'boolean',unit:'unitless'},boolean_output:{label:'Answer, in declared order',type:'boolean',unit:'unitless'},
});
export function descriptor(key, descriptors=DESCRIPTORS) {
  const d=descriptors[key];
  if(!d || !['label','type','unit'].every(k=>typeof d[k]==='string'&&d[k].length)) throw new Error(`Missing value descriptor: ${key}`);
  return freeze({label:d.label,type:d.type,unit:d.unit});
}
export function count(value) {
  if(typeof value==='string' && /^(0|[1-9][0-9]*)$/.test(value)) return BigInt(value).toLocaleString('en-US');
  if(typeof value!=='number' || !Number.isSafeInteger(value) || value<0) throw new Error('Expected an exact count');
  return value.toLocaleString('en-US');
}
const integer=(x,max=Number.MAX_SAFE_INTEGER)=>Number.isSafeInteger(x)&&x>=0&&x<=max;
const id=x=>typeof x==='string'&&/^(0|[1-9][0-9]*)$/.test(x)&&x.length<=100;
const next=x=>(BigInt(x)+1n).toString();
// Frozen domains of the shipped factory, independent of producer-supplied counts.
const TOTALS=Object.freeze({synthesis:6144,inventory:864,search:16,money:1});
const MONEY_FIELDS=Object.freeze(['amount','fee_bps','reserve_in','reserve_out']);
const CLASSES=Object.freeze(['Accept','Reject','CommittedFailure','Refused']);
const text=x=>typeof x==='string'&&x.length>0;
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
const plain=x=>{try{return typeof JSON.stringify(x)==='string';}catch{return false;}};
function own(x) { return structuredClone(x); }
function freeze(x) { if(x && typeof x==='object'&&!Object.isFrozen(x)){Object.values(x).forEach(freeze);Object.freeze(x);}return x; }
export function sameIdentity(a,b) {return !!a&&!!b&&['epoch','run','request'].every(k=>id(a[k])&&a[k]===b[k]);}
// Ordinary finite-contract quotient: identity equality is explicit input data.
// Selection: 0 absent, 1 waiting, 2 received. Masks name the four view regions.
export function control(pre,event,matches,ready) {
  const post={...pre};
  if(event===0&&ready&&pre.phase!==1) return {phase:1,selection:0,mask:15};
  if(event===6)return {phase:0,selection:0,mask:15};
  const current=matches.slice(0,3).every(Boolean);
  if(event===2&&current&&pre.phase===1)post.phase=2;
  if(event===3&&pre.phase===2)post.selection=1;
  if(event===4&&current&&matches[3]&&pre.phase===2&&pre.selection===1)post.selection=2;
  if(event===5&&current&&pre.phase===1)post.phase=3;
  return post;
}
export function claimGate(checked,total,complete,timeout,scope) {return complete===true&&timeout===false&&checked===total&&scope==='finite checked';}
export function evidence(checked,total,complete=false) {
  if(!integer(total)||!integer(checked,total)||typeof complete!=='boolean'||(complete&&checked!==total))throw new Error('Invalid evidence counts');
  return freeze({level:complete?'finite checked':'tested',checked,total,complete,verdict:claimGate(checked,total,complete,false,complete?'finite checked':'tested')?'complete':null});
}
export function initial(epoch='0',preferences={motion:'reduce',theme:'light',width:1440}) {
  if(!id(epoch))throw new Error('Invalid session epoch');
  const defaults={amount:'1000',fee_bps:'30',reserve_in:'10000',reserve_out:'20000'};
  const tasks=Object.fromEntries(ACTS.map(act=>[act,{phase:'idle',draft:act==='money'?defaults:act==='inventory'?{mutant:'none'}:{},submitted:null,result:null,candidates:[],chunks:[],evidence:evidence(0,0),elapsed:null,error:null}]));
  return freeze({act:null,phase:'idle',epoch,run:'0',request:'0',identity:null,selection:null,tasks,preferences:own(preferences)});
}
function sequence(values,length) {return Array.isArray(values)&&values.length===length&&Array.from(values).every(value=>value!==undefined);}
function validateResult(act,r,task) {
  if(!r||r.act!==act)throw new Error('Result belongs to another act');
  if(!plain(r))throw new Error('Result is not a plain technical record');
  if(act==='money'){
    if(!sequence(r.input,4)||!sequence(r.outputs,6)||typeof r.outputs[5]!=='boolean'||!sequence(r.overflow?.input,4))throw new Error('Invalid money result');
    for(const v of [...r.input,...r.outputs.slice(0,5),...r.overflow.input])if(typeof v!=='string'||!/^(0|[1-9][0-9]*)$/.test(v))throw new Error('Wide integer must be a decimal string');
    if(r.input.some((value,i)=>value!==task.submitted[MONEY_FIELDS[i]]))throw new Error('Money result belongs to another request');
    if(!integer(r.overflow.steps))throw new Error('Invalid money result');
    return evidence(1,1,false); // A tuple and a probe never establish a domain theorem.
  }
  const total=TOTALS[act];
  if(r.domain!==total||task.evidence.total!==total)throw new Error('Result does not cover the frozen task domain');
  if(r.passed!==total)throw new Error('Passing count does not support the result claim');
  const bits=(values,length)=>sequence(values,length)&&values.every(v=>v===0||v===1);
  if(act==='inventory') {
    if(r.mutant!==task.submitted.mutant||!integer(r.differences,total)||!integer(r.mutant_passed,total)||!integer(r.law_checks))throw new Error('Invalid inventory evidence');
    if(!r.grid||r.grid.mutant!==r.mutant||!sequence(r.grid.cells,total))throw new Error('Incomplete result grid');
    if(r.grid.cells.some((c,i)=>{
      const input=c?.input;
      if(!sequence(input,5))return true;
      const [available,reserved,action,quantity,authorized]=input;
      if(!integer(available,5)||!integer(reserved,5)||!integer(action,153)||action<150||!integer(quantity,3)||quantity<1||!integer(authorized,1))return true;
      const ordinal=(((available*6+reserved)*4+action-150)*3+quantity-1)*2+authorized;
      return c.ordinal!==i||ordinal!==i||typeof c.different!=='boolean';
    }))throw new Error('Unordered or incomplete result grid coverage');
    const cells=r.grid.cells;
    if(cells.some(c=>!CLASSES.includes(c.class)||!CLASSES.includes(c.mutant_class)))throw new Error('Unknown inventory decision class');
    if(cells.some(c=>c.class!==c.mutant_class&&!c.different))throw new Error('A changed decision is not marked as a difference');
    if(cells.filter(c=>c.different).length!==r.differences||(r.mutant==='none'&&r.differences!==0))throw new Error('Grid counts disagree');
    if(cells.filter(c=>c.mutant_class!=='Refused').length!==r.mutant_passed||cells.filter(c=>c.class!=='Refused').length!==r.passed)throw new Error('Passing counts disagree with the grid');
    // The ringed cell is the one the review's witness names, and it is a difference; neither exists without one.
    const first=r.grid.first_ordinal,w=r.first_difference,decision=d=>{const {laws,...rest}=d??{};return JSON.stringify(rest);};
    if(r.differences===0?first!==null||w!==null||r.status!=='equivalent over the full domain':r.status!=='distinguished'||!integer(first,total-1)||!w)throw new Error('Difference witness disagrees with the grid');
    if(first!==null&&(!cells[first].different||!same(w.input,cells[first].input)||w.original?.class!==cells[first].class||w.mutant?.class!==cells[first].mutant_class||decision(w.original)===decision(w.mutant)))throw new Error('The ringed cell is not the witnessed difference');
    const names=r.names;
    if(!sequence(names?.inputs,5)||names.inputs.some(input=>!text(input?.name))||[150,151,152,153].some(v=>!text(names.variants?.[v]))||['fields','reasons','channels','laws'].some(key=>!names[key]||typeof names[key]!=='object'))throw new Error('Missing inventory names');
  }
  if(act==='synthesis') {
    if(!sequence(r.candidates,8)||r.assignments!==r.candidates.length)throw new Error('Incomplete candidate coverage');
    if(r.candidates.some((c,i)=>c.candidate!==i||!bits(c.choices,3)||c.choices.some((v,bit)=>v!==(i>>(2-bit)&1))||!integer(c.checked,total)||c.checked===0||!['selected','refuted'].includes(c.status)))throw new Error('Invalid candidate evidence');
    const selected=r.candidates.filter(c=>c.status==='selected');
    if(selected.length!==1||selected[0].checked!==total||selected[0].witness!==null)throw new Error('No complete selected candidate');
    if(r.candidates.some(c=>c.status==='refuted'&&(!c.witness||!Array.isArray(c.witness.input)||c.witness.input.length!==12||!Array.isArray(c.witness.output))))throw new Error('Missing candidate counterexample');
    if(JSON.stringify(r.first_counterexample)!==JSON.stringify(r.candidates.find(c=>c.status==='refuted')?.witness))throw new Error('Candidate counterexample disagrees');
    const w=r.first_counterexample;
    if(!sequence(r.inputs,12)||r.inputs.some(name=>!text(name))||!text(r.names?.sums?.TreasuryAction?.[w.input[0]])||!text(r.names?.reason_ranks?.[w.output[0]]))throw new Error('Missing synthesis names');
  }
  if(act==='search') {
    if(!sequence(r.table,total)||r.table.some((row,i)=>!bits(row.input,4)||row.input.some((v,bit)=>v!==(i>>(3-bit)&1))||!bits(row.output,3)))throw new Error('Incomplete search replay coverage');
    if(!integer(r.before_nodes)||!integer(r.after_nodes)||!integer(r.work))throw new Error('Invalid search evidence');
    const w=r.first_difference;
    if(!integer(w?.ordinal,total-1)||!same(w.input,r.table[w.ordinal].input)||!same(w.original,r.table[w.ordinal].output)||!bits(w.swapped,3)||same(w.swapped,w.original))throw new Error('Missing refused-proposal witness');
  }
  return evidence(total,total,true);
}
function inputIssue(act,draft={}) {
  if(typeof act!=='string'||!ACTS.includes(act.split('/')[0]))return 'Choose a declared factory act.';
  if(draft===null||typeof draft!=='object'||Array.isArray(draft))return 'Supply the declared input fields.';
  if(act.startsWith('inventory/'))return ['none','m0002','m0018','m0040','m0049','m0054','m0061'].includes(act.split('/')[1])&&draft.mutant===act.split('/')[1]?null:'Choose a declared inventory variant.';
  if(act==='money') {
    for(const key of MONEY_FIELDS){
      const maximum=key==='fee_bps'?10000:3000000000,minimum=key.startsWith('reserve')?1:0;
      if(typeof draft[key]!=='string'||!/^(0|[1-9][0-9]*)$/.test(draft[key])||BigInt(draft[key])>BigInt(maximum)||BigInt(draft[key])<BigInt(minimum))return `${key.replaceAll('_',' ')} must be a whole number from ${count(minimum)} to ${count(maximum)}.`;
    }
    return null;
  }
  return ['synthesis','search'].includes(act)?null:'Choose a declared factory act.';
}
function ready(act,draft) {return inputIssue(act,draft)===null;}
export function abstractState(state) {return {phase:PHASES.indexOf(state.phase),selection:state.selection?(state.selection.result?2:1):0,mask:15};}
export function step(state,event) {
  const intents=[];
  if(!event||!EVENTS.includes(event.type))return {state,intents,view:view(state)};
  if(event.type==='DraftChanged') {
    if(!ACTS.includes(event.act))return {state,intents,view:view(state)};
    const updated=freeze({...state,tasks:{...state.tasks,[event.act]:{...state.tasks[event.act],draft:own(event.draft??{})}}});
    return {state:updated,intents,view:view(updated)};
  }
  if(event.type==='PreferencesChanged') {
    const preferences={...state.preferences,...own(event.preferences)};
    const updated=freeze({...state,preferences});return {state:updated,intents,view:view(updated)};
  }
  if(event.type==='Cancel') {
    const tasks={...state.tasks};
    if(state.act)tasks[state.act]={...tasks[state.act],phase:'idle',error:null,evidence:evidence(0,tasks[state.act].evidence.total),result:null,chunks:[],candidates:[]};
    const updated=freeze({...state,phase:'idle',epoch:next(state.epoch),identity:null,selection:null,tasks});
    return {state:updated,intents:[{kind:'cancel'}],view:view(updated)};
  }
  if(event.type==='Start') {
    const draft=own(event.draft??{});
    if(typeof event.act!=='string'||control(abstractState(state),0,[],ready(event.act,draft)).phase!==1||state.phase==='running')return {state,intents,view:view(state)};
    const act=event.act.split('/')[0],run=next(state.run),request=next(state.request),identity={epoch:state.epoch,run,request};
    const total=TOTALS[act];
    const task={phase:'running',draft,submitted:draft,result:null,candidates:[],chunks:[],evidence:evidence(0,total),elapsed:null,error:null};
    const updated=freeze({...state,act,phase:'running',run,request,identity,selection:null,tasks:{...state.tasks,[act]:task}});
    return {state:updated,intents:freeze([{kind:'run',identity,act:event.act,options:draft}]),view:view(updated)};
  }
  if(event.type==='Select') {
    const task=state.tasks.inventory;
    if(state.act!=='inventory'||!sameIdentity(event.identity,state.identity)||control(abstractState(state),3,[],true).selection!==1||!integer(event.ordinal,863)||!task.result)return {state,intents,view:view(state)};
    const request=next(state.request),identity={...state.identity,request},selection={ordinal:event.ordinal,identity,result:null,error:null};
    const updated=freeze({...state,request,selection});
    return {state:updated,intents:freeze([{kind:'inspect',identity,mutant:task.result.mutant,ordinal:event.ordinal}]),view:view(updated)};
  }
  const cell=event.type==='CellResult'||(event.type==='Fail'&&event.ordinal!==undefined);
  const target=cell?state.selection?.identity:state.identity;
  if(!sameIdentity(event.identity,target)||(cell&&event.ordinal!==state.selection?.ordinal))return {state,intents,view:view(state)};
  if(cell) {
    if(control(abstractState(state),4,[true,true,true,true],true).selection!==2||state.selection.result||state.selection.error)return {state,intents,view:view(state)};
    const expected=state.tasks.inventory.result.grid.cells[event.ordinal].input;
    if(event.type==='CellResult'&&JSON.stringify(event.result?.input)!==JSON.stringify(expected))throw new Error('Cell report belongs to another input');
    const selection={...state.selection,result:event.type==='CellResult'?own(event.result):null,error:event.type==='Fail'?String(event.message):null};
    const updated=freeze({...state,selection});return {state:updated,intents,view:view(updated)};
  }
  if(state.phase!=='running')return {state,intents,view:view(state)};
  const task=state.tasks[state.act];let changes;
  if(event.type==='Progress') {
    if(event.candidate) {
      const c=event.candidate;
      if(state.act!=='synthesis'||!integer(c.candidate,7)||c.candidate!==task.candidates.length||!integer(c.checked,6144)||!['selected','refuted'].includes(c.status))throw new Error('Invalid candidate progress');
      changes={candidates:[...task.candidates,own(c)],evidence:evidence(c.checked,6144)};
    } else {
      const p=event.progress;
      if(state.act!=='inventory'||!p||p.start!==task.chunks.length*48||p.cells?.length!==48||p.cells.some((c,i)=>c.ordinal!==p.start+i)||p.mutant!==task.submitted.mutant)throw new Error('Invalid inventory progress');
      changes={chunks:[...task.chunks,own(p)],evidence:evidence(p.start+p.cells.length,864)};
    }
  } else if(event.type==='Result') {
    if(!Number.isFinite(event.elapsed)||event.elapsed<0)throw new Error('Invalid elapsed time');
    const result=own(event.result),checked=validateResult(state.act,result,task);
    changes={phase:PHASES[control(abstractState(state),2,[true,true,true,true],true).phase],result,evidence:checked,elapsed:event.elapsed};
  } else if(event.type==='Fail') {
    changes={phase:PHASES[control(abstractState(state),5,[true,true,true,true],true).phase],result:null,error:String(event.message),evidence:evidence(task.evidence.checked,task.evidence.total)};
  } else return {state,intents,view:view(state)};
  const updated=freeze({...state,epoch:event.type==='Fail'?next(state.epoch):state.epoch,phase:changes.phase??state.phase,tasks:{...state.tasks,[state.act]:{...task,...changes}}});
  return {state:updated,intents,view:view(updated)};
}
function metric(key,value,tone) {const d=descriptor(key);return {key,value,label:d.label,type:d.type,unit:d.unit,tone};}
function resultView(act,task) {
  const r=task.result;
  if(!r)return {summary:task.evidence.total?`${count(task.evidence.checked)} of ${count(task.evidence.total)} checked`:'No inputs checked yet.',metrics:[metric('checked',count(task.evidence.checked))]};
  const metrics=[];let summary='';
  if(act==='inventory') {
    metrics.push(metric('checked',count(r.domain)),metric('passed',count(r.mutant==='none'?r.passed:r.mutant_passed),'good'));
    metrics.push(r.mutant==='none'?metric('law_checks',count(r.law_checks)):metric('differences',count(r.differences),r.differences?'warn':'good'));
    summary=r.mutant==='none'?`${count(r.passed)} of ${count(r.domain)} decisions satisfy every applicable law.`:r.differences===0?`This change alters no decision on any of the ${count(r.domain)} inputs: the two versions are equivalent.`:`The bug changes ${count(r.differences)} of ${count(r.domain)} decisions. One of them is ringed in the grid, and its two decisions are compared in the panel.`;
  } else if(act==='synthesis') {
    const selected=r.candidates.find(c=>c.status==='selected');
    metrics.push(metric('assignments',count(r.assignments)),metric('refuted',count(r.candidates.filter(c=>c.status==='refuted').length),'warn'),metric('passed',`${count(r.passed)} / ${count(r.domain)}`,'good'));
    summary=`Order ${selected.candidate+1} is chosen. ${count(r.passed)} of ${count(r.domain)} declared inputs pass.`;
  } else if(act==='search') {
    metrics.push(metric('nodes',`${r.before_nodes} → ${r.after_nodes}`),metric('passed',`${count(r.passed)} / ${count(r.domain)}`,'good'),metric('work',count(r.work)));
    summary=`The selected program gives the same answers on ${count(r.domain)} of ${count(r.domain)} declared inputs.`;
  } else summary='One supplied swap tuple and one wider-domain overflow probe checked.';
  metrics.push(metric('elapsed',`${task.elapsed.toFixed(0)} ms`));
  return {summary,metrics};
}
export function view(state) {
  const tasks=Object.fromEntries(ACTS.map(act=>{
    const t=state.tasks[act],content=resultView(act,t),issue=inputIssue(act==='inventory'?`inventory/${t.draft.mutant}`:act,t.draft);
    return [act,{...t,...content,regions:REGIONS,record:t.result,verdict:t.evidence.verdict==='complete'?'finite checked':null,
      status:t.phase==='running'?`${count(t.evidence.checked)} of ${count(t.evidence.total)} checked — running in a background worker…`:t.phase==='error'?`Stopped: ${t.error}. No verdict.`:issue?`Input bounds: ${issue}`:t.phase==='done'?`Done in ${t.elapsed.toFixed(0)} ms, in this browser.`:'Ready. No result yet.'}];
  }));
  return freeze({tasks,act:state.act,phase:state.phase,selection:state.selection,busy:state.phase==='running',preferences:state.preferences,
    actions:ACTS.map(act=>({act,enabled:state.phase!=='running'&&ready(act==='inventory'?`inventory/${state.tasks.inventory.draft.mutant}`:act,state.tasks[act].draft)})),gridEnabled:state.act==='inventory'&&state.phase==='done'});
}
