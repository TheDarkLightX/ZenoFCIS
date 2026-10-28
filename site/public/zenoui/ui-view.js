// The pure view of a synthesized UI: the checked component tree plus the reducer's view data in, a
// view tree out, keyed by tree path. No DOM, globals, time or randomness: it runs in Node, and the
// UI laws check it. The renderer only maps this tree to elements.
import {display,formFields,amountText,editText} from './app-contract.js';
import {scopeCommands,scopeInputs,applies} from './ui-grammar.js';
const RESULT={Accept:['Accepted','success'],Reject:['Refused','danger'],CommittedFailure:['Recorded as failed','warning'],Refused:['Not accepted','danger']};
const VERDICT={done:['success','Done.'],refused:['danger','Not accepted.'],error:['danger','Stopped.'],running:['info','Checking.']};
// Exact ratio of two decimal strings as text with up to `places` decimals, rounded down; no floats.
export function ratio(numerator, denominator, places=4) {
  const n=BigInt(numerator),d=BigInt(denominator);if(d===0n)return null;
  const scaled=n*10n**BigInt(places)/d,whole=scaled/10n**BigInt(places),part=(scaled%10n**BigInt(places)).toString().padStart(places,'0').replace(/0+$/,'');
  return part?`${whole}.${part}`:`${whole}`;
}
function walk(node, path, visit) { visit(node,path); (node.children??[]).forEach((c,i)=>walk(c,`${path}.${i}`,visit)); }
export function presentationView(app, data, options={}) {
  const server=!!options.server,req=app.presentation.requirements,nodes={};
  const field=path=>app.fields.find(f=>f.path===path);
  const resolve=scope=>scope==='selected'||scope.startsWith('group-')?data.selectedCommand:scope;
  const command=id=>app.commands.find(c=>c.id===id);
  const action=id=>data.actions.find(a=>a.id===id);
  // One primary action: the confirmation while it is open, otherwise the first possible form in tree order.
  const scopes=[];walk(app.presentation.ast,'$',n=>{if(n.kind==='scope')scopes.push(n.scope);});
  // A shared form is possible when any of its commands is; its choice then picks a direction.
  const servedBy=scope=>scopeCommands(app,req,scope)??[];
  const readyScope=scopes.find(s=>servedBy(s).includes(data.selectedCommand)&&action(data.selectedCommand)?.enabled)??scopes.find(s=>servedBy(s).some(c=>action(c)?.enabled))??null;
  const primaryScope=data.confirmation?null:readyScope??scopes.find(s=>servedBy(s).some(c=>action(c)?.available))??null;
  const byIndex=new Map(data.state.map(s=>[s.index,s]));
  const idioms=new Map();walk(app.presentation.ast,'$',n=>{if(n.kind==='idiom')idioms.set(n.scope,n.idiom);});
  const quote=id=>(data.projectionsByCommand[id]??[]).find(p=>p.kind==='accepted-decrease');
  // The assets a swap direction pays and receives: the units of its out reserve and of the other
  // direction's out reserve.
  const assets=(scope,id)=>{
    if(idioms.get(scope)!=='swap-card')return null;
    const served=scopeCommands(app,req,scope),p=req.projections.find(p=>p.kind==='accepted-decrease'&&applies(p,served));
    const other=served.find(c=>c!==id),out=field(p.fieldByCommand[id]);
    return {receive:out.unit,pay:other?field(p.fieldByCommand[other]).unit:null};
  };
  walk(app.presentation.ast,'$',(node,path)=>{
    switch(node.kind){
      case 'commands':
        {const ready=data.confirmation?null:readyScope,served=ready?servedBy(ready):[],id=served.includes(data.selectedCommand)&&action(data.selectedCommand)?.enabled?data.selectedCommand:served.find(c=>action(c)?.enabled);
        nodes[path]={heading:'Your next action',empty:!id,emptyText:data.actions.some(a=>a.tone==='pending')?'Checking the current requests.':'No request is ready to review. Check the fields or choose another action.',
          next:id?{scope:ready,command:id,label:command(id).label,select:ready==='selected'||ready.startsWith('group-')}:null};break;}
      case 'disclosure':
        nodes[path]={label:node.label==='Evidence'?'Technical record':'More options'};break;
      case 'scope': {
        const served=scopeCommands(app,req,node.scope),id=resolve(node.scope),a=action(id);
        const label=served.length>1?(node.children[0]?.idiom==='swap-card'?'Swap':node.scope==='selected'&&node.children[0]?.idiom!=='approval-decision'?command(id).label:served.map(c=>command(c).label).join(' or ')):command(id).label;
        nodes[path]={command:id,label,available:served.some(c=>action(c)?.available),reason:a?.reason??null,primary:node.scope===primaryScope,commands:served,
          ready:!!a?.enabled,preview:!a?.enabled?null:{changes:a.changes??[]}};break;
      }
      case 'idiom': {
        if(node.idiom==='scalar-edit'||node.idiom==='approval-decision'){
          // The child display is the single checked observation of current state.
          nodes[path]={};
          break;
        }
        // Swap card: the amount paid is an input, "You receive" is the core's quote, the rate and
        // price impact are exact ratios of that quote, the amount and the current reserves.
        const id=resolve(node.scope),inputs=scopeInputs(app,req,node.scope),amountIn=inputs.map(field).find(f=>f.role==='amount-in'),minOut=inputs.map(field).find(f=>f.role==='min-out');
        const p=req.projections.find(p=>p.kind==='accepted-decrease'&&applies(p,scopeCommands(app,req,node.scope)));
        const out=field(p.fieldByCommand[id]),other=scopeCommands(app,req,node.scope).find(c=>c!==id),inReserve=other?field(p.fieldByCommand[other]):null;
        const amount=data.drafts?.[id]?.[amountIn.index]??'',q=quote(id)?.value??null;
        const reserveIn=inReserve?byIndex.get(inReserve.index)?.raw:null,reserveOut=byIndex.get(out.index)?.raw;
        let impact=null;
        if(q!==null&&reserveIn&&reserveOut&&/^[1-9][0-9]*$/.test(amount)&&BigInt(reserveOut)>0n){
          // impact = 1 - (q / amount) / (reserveOut / reserveIn), in basis points, rounded down.
          const bps=10000n-BigInt(q)*BigInt(reserveIn)*10000n/(BigInt(amount)*BigInt(reserveOut));
          impact={percent:ratio(bps>0n?bps:0n,100n,2),high:bps>=500n};
        }
        const a=action(id);
        // The rate compares whole tokens: each side's smallest units are scaled by its decimal places.
        const scale=f=>10n**BigInt(f?.decimals??0),minimum=minOut?data.drafts?.[id]?.[minOut.index]??'':'';
        nodes[path]={payAsset:inReserve?.unit??amountIn.unit,receiveAsset:out.unit,amount,quote:q,quoteText:q===null?null:amountText(out,q),
          rate:q!==null&&/^[1-9][0-9]*$/.test(amount)?`1 ${inReserve?.unit??amountIn.unit} = ${ratio(BigInt(q)*scale(amountIn),BigInt(amount)*scale(out))} ${out.unit}`:null,impact,
          minimum:minOut?{value:minimum,text:amountText(minOut,minimum),unit:out.unit}:null,slippage:options.slippage??null,
          state:!a?.available?'unavailable':!/^[1-9][0-9]*$/.test(amount)?'empty':a.tone};
        break;
      }
      case 'input': {
        // In a swap card an amount carries the asset of its direction instead of a generic unit.
        const f=field(node.field),id=resolve(node.scope),value=data.drafts?.[id]?.[f.index]??f.initial,swap=assets(node.scope,id);
        const unit=swap&&f.role==='amount-in'?swap.pay:swap&&f.role==='min-out'?swap.receive:f.unit==='unitless'?null:f.unit;
        nodes[path]={id:`command-${node.scope}-field-${f.index}`,command:id,label:f.label,value:editText(f,value),decimal:!!f.decimals,unit,
          choices:f.options.length?f.options.map(o=>({value:o.value,label:o.label})):null,
          help:f.domain.kind==='int'&&!f.options.length?`${f.decimals&&BigInt(f.domain.min)<=1n?'Up to ':`${amountText(f,f.domain.min)} to `}${amountText(f,f.domain.max)}${f.unit==='unitless'||unit!==f.unit?'':` ${f.unit}`}`:null,
          disabled:data.phase==='running'};break;
      }
      case 'selector': {
        const served=scopeCommands(app,req,node.scope);
        nodes[path]={id:'command-direction',label:idioms.get(node.scope)==='approval-decision'?'Decision':idioms.get(node.scope)==='swap-card'?'Direction':app.fields[app.action].label,value:data.selectedCommand,options:served.map(c=>({value:c,label:command(c).label})),disabled:data.phase==='running'};break;
      }
      case 'projection': {
        const id=resolve(node.scope),p=(data.projectionsByCommand[id]??[]).find(x=>x.id===node.projection),declared=req.projections.find(x=>x.id===node.projection);
        const f=declared?.fieldByCommand?.[id]?field(declared.fieldByCommand[id]):null;
        nodes[path]={label:p?.label??'',value:p?.value==null?null:f?amountText(f,p.value):p.value,unit:p?.unit??'',pending:p?.value==null};break;
      }
      case 'review': {
        // A swap card's button names what to do next; other forms say "Review".
        const id=resolve(node.scope),a=action(id),idiom=idioms.get(node.scope),swap=idiom==='swap-card';
        const amount=swap?data.drafts?.[id]?.[scopeInputs(app,req,node.scope).map(field).find(f=>f.role==='amount-in').index]??'':'';
        // A minimum of 0 is never sent once the quote is above 0: from the slippage setting it means the
        // amount is too small to protect; typed by hand it must be raised.
        const custom=options.slippage?.value==='custom',guard=swap&&a?.tone==='unprotected';
        const text=!swap?(idiom==='scalar-edit'?'Review update':idiom==='approval-decision'?'Review decision':'Review'):!a?.available?'Not available':!/^[1-9][0-9]*$/.test(amount)?'Enter an amount':guard?(custom?'Set a minimum above 0':'Amount too small to protect'):a.tone==='ready'?'Review swap':a.tone==='refused'?'Swap not possible':'Checking';
        const feedback=!guard?a?.explanation??'':custom?'A minimum of 0 accepts any amount. Enter a minimum above 0 under More options.'
          :`At ${options.slippage?.value}% maximum slippage the minimum to receive rounds down to 0, so this swap would accept any amount. Pay more, or set the minimum yourself under More options.`;
        nodes[path]={command:id,text,aria:swap?`${text}: ${command(id).label}`:`Review ${command(id).label}`,enabled:!!a?.enabled,feedback,tone:a?.tone??'pending',hidden:!swap&&!a?.available,primary:node.scope===primaryScope,block:swap};
        break;
      }
      case 'confirmation': {
        const c=data.confirmation;
        if(!c){nodes[path]={open:false};break;}
        // A swap names its tokens: what it pays in is what the other direction pays out.
        const flow=req.projections.find(p=>p.kind==='accepted-decrease'&&p.fieldByCommand?.[c.command]),back=flow&&Object.keys(flow.fieldByCommand).find(k=>flow.fieldByCommand[k]!==flow.fieldByCommand[c.command]);
        const asset=f=>{const role=app.fields[f.index].role;return !flow?null:role==='min-out'?field(flow.fieldByCommand[c.command]).unit:role==='amount-in'&&back?field(flow.fieldByCommand[back]).unit:null;};
        const shown=f=>asset(f)?`${amountText(app.fields[f.index],f.raw)} ${asset(f)}`:f.value;
        const rows=c.fields.filter(f=>f.part==='command'&&f.used).map(f=>({label:f.label,value:shown(f),role:app.fields[f.index].role}));
        const who=[...c.fields.filter(f=>f.part==='context').map(f=>({label:f.label,value:f.value})),...(server?[]:[{label:'Your name',value:c.principal}])];
        const quoted=(c.projections??[]).find(p=>p.kind==='accepted-decrease'&&p.value!==null),declared=quoted&&req.projections.find(x=>x.id===quoted.id);
        const receive=quoted&&{...quoted,value:amountText(field(declared.fieldByCommand[c.command]),quoted.value)};
        // The consequence sentence uses only the command, its exact inputs and the core's preview.
        const pay=rows.find(r=>r.role==='amount-in'),least=rows.find(r=>r.role==='min-out');
        const consequence=receive&&pay?`${c.label}: you pay ${pay.value} and you receive about ${receive.value} ${receive.unit}${least?`, with a minimum of ${least.value}`:''}.`
          :receive?`${c.label}: you receive about ${receive.value} ${receive.unit}${rows.length?`, for ${rows.map(r=>`${r.label.toLowerCase()} ${r.value}`).join(' and ')}`:''}.`
          :c.changes.length?`${c.label}: ${c.changes.map(x=>`${x.label} becomes ${x.after}`).join('; ')}.`:`${c.label}: no state changes are visible in this preview.`;
        nodes[path]={open:true,heading:`Check and confirm: ${c.label}`,consequence,changes:c.changes,request:[{label:'Command',value:c.label},...rows.map(({label,value})=>({label,value}))],who,
          quote:receive?[{label:receive.label,value:`${receive.value} ${receive.unit}`}]:[],
          revision:c.revision,complete:c.fields.map(f=>({label:f.label,muted:!f.used&&f.part==='command',value:f.part==='state'?`${f.value} (now)`:f.part==='command'&&!f.used?`${f.value} (not used by ${c.label})`:f.part==='command'?shown(f):f.value})),
          record:{contract:c.contract,principal:c.principal,revision:c.revision},confirmLabel:`Confirm ${c.label}`};
        break;
      }
      case 'status': {
        const unknown=data.phase==='error'&&data.delivery==='unknown';
        const [tone,title]=unknown?['danger','Outcome unknown.']:VERDICT[data.phase]??['neutral',null];
        nodes[path]={tone,title,message:unknown?'No confirmed result is available.':data.message,delivery:options.delivery??'',pending:options.pending??''};break;
      }
      case 'acting':
        nodes[path]={server,label:server?'Your access':'Acting as',
          // A lone named party reads alone ("Buyer"); a yes/no or a number keeps its label.
          text:data.context.length?data.context.map(c=>data.context.length>1||app.fields[c.index].domain.kind!=='sum'?`${c.label}: ${c.value}`:c.value).join(' · '):'Signed-in user',
          principal:data.principal,fields:server?[]:app.fields.filter(f=>f.source==='context').map(f=>({id:`context-field-${f.index}`,index:f.index,label:f.label,value:data.drafts?.[app.commands[0].id]?.[f.index]??f.initial,
            choices:f.options.length?f.options.map(o=>({value:o.value,label:o.label})):null,help:f.options.length?null:`${f.domain.min} to ${f.domain.max}`}))};
        break;
      case 'display': {
        const items=node.fields.map(p=>byIndex.get(field(p).index)).filter(Boolean);
        if(node.idiom==='table-list'){const table=app.tables.find(t=>t.entries[0]===node.fields[0]);nodes[path]={label:table.label,entries:items.map(s=>({index:s.index,label:s.label,value:s.value}))};}
        else if(node.idiom==='stepper'){const s=items[0],f=s&&field(app.fields[s.index].path);
          nodes[path]=s?{label:s.label,index:s.index,steps:f.stages.map((level,i)=>{const at=f.stages.findIndex(l=>l.includes(s.raw));return {label:i===at?display(f,s.raw):level.map(v=>display(f,v)).join(' or '),state:i<at?'done':i===at?'current':'todo'};}),current:s.value}:{empty:true};}
        else if(node.idiom==='metrics')nodes[path]={items:items.map(s=>({index:s.index,label:s.label,value:s.value,raw:s.raw,unit:app.fields[s.index].unit,named:s.value!==(app.fields[s.index].unit==='unitless'?s.raw:`${s.raw} ${app.fields[s.index].unit}`)}))};
        else if(node.idiom==='lanes')nodes[path]={lanes:(app.lanes??[]).flatMap(g=>{const s=byIndex.get(g.lane);if(!s)return [];
          return [{index:g.lane,title:g.title,status:s.value,rows:g.members.filter(m=>byIndex.has(m.index)).map(m=>({index:m.index,label:m.label,value:byIndex.get(m.index).value}))}];})};
        else nodes[path]={rows:items.map(s=>({index:s.index,label:s.label,value:s.value,badge:s.role!=='identity'&&s.role!=='amount',tone:s.role==='flag'&&s.raw==='1'?'info':'neutral'}))};
        break;
      }
      case 'evidence': {
        const r=node.region;
        nodes[path]=r==='state'?{heading:'State',rows:data.state.map(s=>({label:s.label,value:s.value}))}
          :r==='metrics'?{text:`Requests checked by the core: ${data.evidence.checked} of ${data.evidence.total}`}
          :r==='comparison'?{text:'The core rechecks every confirmed request before it decides.'}
          :r==='ledger'?{heading:'History',entries:data.ledger.map((e,i)=>{const [text,tone]=RESULT[e.result]??[e.result,'neutral'];return {step:String(i+1),command:e.command,result:text,tone};}),empty:'Nothing has happened yet. Each decision appears here.'}
          :{record:options.record??{}};
        break;
      }
      default: nodes[path]={};
    }
  });
  return {nodes,phase:data.phase,revision:options.revision??null,archetype:data.archetype??null,title:app.title,
    lede:server?'Review your next action before you confirm it.':'A local preview. Requests are checked, and nothing is saved or sent.'};
}
export {formFields};
