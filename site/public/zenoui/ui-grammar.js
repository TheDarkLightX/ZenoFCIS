// Independent whole-tree acceptance. Construction and caches confer no authority.
import {idiom,displayFits,swapFits} from './ui-idioms.js';
export const GRAMMAR = 'zenoui/component-grammar/3';
export const CHECKER = 'zenoui/component-checker/3';
export const OBJECTIVE = 'zenoui/component-objective/3';
export const EXPERT_REGIONS = Object.freeze(['state','metrics','comparison','ledger','record']);
const containers=['page','stack','card','grid','disclosure','commands','scope','idiom'];
const keys={page:['kind','children'],stack:['kind','children'],card:['kind','children'],grid:['kind','children'],disclosure:['kind','label','children'],commands:['kind','children'],scope:['kind','scope','children'],idiom:['kind','idiom','scope','children'],input:['kind','scope','field'],projection:['kind','scope','projection'],selector:['kind','scope'],review:['kind','scope'],confirmation:['kind'],status:['kind'],evidence:['kind','region'],acting:['kind'],display:['kind','idiom','fields']};
// Version 1 projections, declared for every command, apply everywhere.
export const applies=(p,commands)=>!p.fieldByCommand||(commands.length>0&&commands.every(c=>Object.hasOwn(p.fieldByCommand,c)));
const selects=(scope,commands)=>scope==='selected'||commands.length>1;
const exact=(value,allowed)=>value&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).length===allowed.length&&allowed.every(k=>Object.hasOwn(value,k));
// Check these signatures independently of the search's fit functions. Metadata
// names an intended presentation; it cannot attest to a policy's behavior.
function editSignature(app,req,commands,required,scope) {
  if(scope==='selected'||commands.length!==1||required.length!==1||req.projections.length!==0)return false;
  const command=app.commands.find(c=>c.id===commands[0]);
  const input=app.fields.find(f=>f.path===required[0]);
  const current=app.fields.find(f=>f.path===input?.stateField);
  const inLane=(app.lanes??[]).some(g=>g.lane===current?.index||g.members.some(m=>m.index===current?.index));
  return command?.uiRole==='update'&&command.consequential===true&&input?.source==='command'&&
    input.role==='requested-value'&&input.domain.kind==='int'&&req.primaryFields.includes(input.path)&&
    current?.source==='state'&&current.domain.kind==='int'&&current.unit===input.unit&&
    (current.decimals??0)===(input.decimals??0)&&!inLane;
}
function approvalSignature(app,req,commands,required) {
  if(app.commands.length!==2||commands.length!==2||required.length!==0||req.projections.length!==0)return false;
  const pair=commands.map(id=>app.commands.find(c=>c.id===id));
  const statuses=app.fields.filter(f=>f.source==='state'&&f.role==='approval-status');
  const status=statuses[0],named=status?.domain.kind==='sum'||status?.domain.kind==='bool'||
    (status?.domain.kind==='int'&&status.values&&Object.keys(status.values).length>0);
  const inLane=(app.lanes??[]).some(g=>g.lane===status?.index||g.members.some(m=>m.index===status?.index));
  return statuses.length===1&&!!named&&!inLane&&pair.every(c=>c?.consequential===true)&&
    pair.filter(c=>c.uiRole==='approve').length===1&&pair.filter(c=>c.uiRole==='reject').length===1;
}
const idiomChildren=(node,field,control)=>node?.children?.length===3&&node.children[0]?.kind==='display'&&
  node.children[0].idiom==='facts'&&node.children[0].fields?.length===1&&node.children[0].fields[0]===field&&
  node.children[1]?.kind===control&&node.children[2]?.kind==='review';
// Commands a scope serves: one command, every command ("selected"), or a declared group.
export function scopeCommands(app, requirements, scope) {
  if (scope==='selected') return app.commands.map(c=>c.id);
  const group=(requirements.groups??[]).find(g=>`group-${g.id}`===scope);
  if (group) return [...group.commands];
  return app.commands.some(c=>c.id===scope)?[scope]:null;
}
// A scope's form inputs: the declared inputs that can change a decision of its commands. Requirements
// version 1 has no relevance and keeps every declared input in every scope.
export function scopeInputs(app, requirements, scope) {
  const commands=scopeCommands(app,requirements,scope),declared=[...requirements.primaryFields,...requirements.detailFields];
  if (!commands) return null;
  if (requirements.format!=='zenoui/requirements/2' || !app.relevant) return declared;
  const sets=commands.map(id=>declared.filter(p=>app.relevant[id].includes(app.fields.find(f=>f.path===p).index)));
  return sets.every(s=>s.join()===sets[0].join())?sets[0]:null;
}
export function validateAst(app, requirements, ast, limits={}) {
  const maxNodes=limits.maxNodes??128,maxDepth=limits.maxDepth??10,v2=requirements.format==='zenoui/requirements/2';
  const failures=[];const fail=(code,path)=>{if(failures.length<32)failures.push({code,path});};
  const summary={nodes:0,depth:0,visibleForms:0,expandedExpertPanels:0,columns:1,scopes:[],primaryCardGaps:0,disclosureInputs:0,duplicateInputs:0,visibleSecondary:0,idiomMisfit:0,summaryAfterCommands:0,inputsBeforeReview:0,acting:0};
  const declared=[...requirements.primaryFields,...requirements.detailFields];
  const inputs=new Map(app.fields.map(f=>[f.path,f]));
  const projections=new Map(requirements.projections.map(p=>[p.id,p]));
  const scopes=new Map(),regions=new Map(),displayed=new Map();let commands=0,confirmations=0,statuses=0,seenCommands=false,displays=0;
  const pending=[{node:ast,path:'$',depth:1,scope:null,hidden:false,card:null,inCommands:false,columns:1,inIdiom:null}];
  while(pending.length){
    const entry=pending.pop(),{node,path,depth,scope,hidden,card,inCommands,columns,inIdiom}=entry;
    summary.nodes++;summary.depth=Math.max(summary.depth,depth);
    if(summary.nodes>maxNodes || depth>maxDepth){fail('tree-budget',path);break;}
    if(!node || !Object.hasOwn(keys,node.kind) || !exact(node,keys[node.kind])){fail('node-schema',path);continue;}
    const kind=node.kind;
    if(kind==='page' && path!=='$')fail('nested-page',path);
    if(path==='$'&&kind!=='page')fail('missing-page',path);
    if(containers.includes(kind)){
      if(!Array.isArray(node.children)||node.children.length<1||node.children.length>64){fail('children-bound',path);continue;}
      if(['stack','grid'].includes(kind)&&node.children.length<2)fail('redundant-container',path);
      if(kind==='disclosure'&&!['Details','Evidence'].includes(node.label))fail('disclosure-label',path);
      let childScope=scope,childCommands=inCommands,childIdiom=inIdiom;
      if(kind==='commands'){
        commands++;childCommands=true;seenCommands=true;
        if(scope!==null||inCommands||hidden)fail('command-region-placement',path);
      }
      if(kind==='scope'){
        if(!inCommands||scope!==null||hidden)fail('scope-placement',path);
        const served=scopeCommands(app,requirements,node.scope);
        if(!served)fail('unknown-scope',path);
        else if(!scopeInputs(app,requirements,node.scope))fail('group-relevance',path);
        if(scopes.has(node.scope))fail('duplicate-scope',path);
        const record={input:new Map(),projection:new Map(),selector:0,review:0,order:[],reviewPosition:-1,idiom:null,idiomNode:null,commands:served??[]};
        scopes.set(node.scope,record);childScope=node.scope;summary.scopes.push(node.scope);summary.visibleForms++;
      }
      if(kind==='idiom'){
        const spec=idiom(node.idiom);
        if(!spec||spec.region!=='commands'||scope===null||node.scope!==scope||inIdiom||hidden)fail('idiom-placement',path);
        else{const record=scopes.get(scope);if(record.idiom)fail('idiom-placement',path);record.idiom=node.idiom;record.idiomNode=node;childIdiom=node.idiom;}
      }
      const childColumns=kind==='grid'?columns*2:columns;summary.columns=Math.max(summary.columns,childColumns);
      for(let i=node.children.length-1;i>=0;i--)pending.push({node:node.children[i],path:`${path}.${i}`,depth:depth+1,scope:childScope,hidden:hidden||kind==='disclosure',card:kind==='card'?path:card,inCommands:childCommands,columns:childColumns,inIdiom:childIdiom});
      continue;
    }
    if(['input','projection','selector','review'].includes(kind)){
      if(scope===null||node.scope!==scope||!scopes.has(scope)){fail('crossed-scope',path);continue;}
      const record=scopes.get(scope);record.order.push({kind,id:node.field??node.projection,hidden});
      if(kind==='input'){
        const field=inputs.get(node.field);
        if(!field||!declared.includes(node.field)||field.source==='state'||field.index===app.action)fail('input-binding',path);
        record.input.set(node.field,(record.input.get(node.field)??0)+1);
        if(requirements.primaryFields.includes(node.field)){
          if(hidden)fail('hidden-primary-input',path);
          if(card===null&&!inIdiom)summary.primaryCardGaps++;
        }else if(hidden)summary.disclosureInputs++;else summary.visibleSecondary++;
        if(!hidden&&record.review===0)summary.inputsBeforeReview++;
      }
      if(kind==='projection'){
        const projection=projections.get(node.projection);
        if(!projection)fail('projection-binding',path);
        record.projection.set(node.projection,(record.projection.get(node.projection)??0)+1);
        if(projection?.kind==='accepted-decrease'){
          if(hidden)fail('hidden-primary-projection',path);
          if(card===null&&!inIdiom)summary.primaryCardGaps++;
        }else if(!hidden)summary.visibleSecondary++;
      }
      if(kind==='selector'){
        record.selector++;
        if(!selects(scope,record.commands)||hidden)fail('selector-contract',path);
      }
      if(kind==='review'){
        record.review++;record.reviewPosition=record.order.length-1;
        if(hidden)fail('hidden-review',path);
      }
    }else{
      if((scope!==null||inCommands)&&!(kind==='display'&&['scalar-edit','approval-decision'].includes(inIdiom)))fail('global-leaf-in-command',path);
      if(kind==='confirmation'){confirmations++;if(hidden)fail('hidden-confirmation',path);}
      if(kind==='status'){statuses++;if(hidden)fail('hidden-status',path);}
      if(kind==='acting'){summary.acting++;if(hidden)fail('hidden-acting',path);}
      if(kind==='display'){
        displays++;
        if(hidden)fail('hidden-display',path);
        if(!displayFits(app,node.idiom,node.fields))fail('display-signature',path);
        for(const p of node.fields??[])displayed.set(p,(displayed.get(p)??0)+1);
        if(seenCommands)summary.summaryAfterCommands++;
        if(node.idiom==='facts'&&!['scalar-edit','approval-decision'].includes(inIdiom))summary.idiomMisfit+=(node.fields??[]).filter(p=>['stage','amount'].includes(inputs.get(p)?.role)).length;
      }
      if(kind==='evidence'){
        if(!EXPERT_REGIONS.includes(node.region))fail('unknown-evidence',path);
        regions.set(node.region,(regions.get(node.region)??0)+1);
        if(!hidden)summary.expandedExpertPanels++;
      }
    }
  }
  if(commands!==1||confirmations!==1||statuses!==1)fail('global-coverage','$');
  if(EXPERT_REGIONS.some(r=>regions.get(r)!==1)||regions.size!==EXPERT_REGIONS.length)fail('evidence-coverage','$');
  const covered=app.commands.map(c=>[...scopes.values()].filter(r=>r.commands.includes(c.id)).length);
  if(covered.some(n=>n!==1))fail('command-coverage','$');
  for(const [scope,record] of scopes){
    const required=scopeInputs(app,requirements,scope)??[];
    if(required.some(p=>record.input.get(p)!==1)||record.input.size!==required.length)fail('input-coverage',scope);
    // A projection belongs in a scope when it is declared for every command the scope serves.
    const wanted=requirements.projections.filter(p=>applies(p,record.commands));
    if(requirements.projections.some(p=>!applies(p,record.commands)&&record.commands.some(c=>Object.hasOwn(p.fieldByCommand??{},c))))fail('projection-scope',scope);
    if(wanted.some(p=>record.projection.get(p.id)!==1)||record.projection.size!==wanted.length)fail('projection-coverage',scope);
    if(record.review!==1||record.selector!==(selects(scope,record.commands)?1:0))fail('control-coverage',scope);
    const primaryPositions=record.order.flatMap((item,i)=>item.kind==='input'&&requirements.primaryFields.includes(item.id)?[i]:[]);
    if(record.order.some((item,i)=>item.kind==='projection'&&projections.get(item.id)?.kind==='accepted-decrease'&&primaryPositions.some(p=>p>i)))fail('preview-dependency',scope);
    if(record.order.some((item,i)=>i>record.reviewPosition&&['input','projection','selector'].includes(item.kind)))fail('review-dependency',scope);
    if(record.idiom==='swap-card'&&!swapFits(app,requirements,record.commands,required))fail('idiom-signature',scope);
    const edit=editSignature(app,requirements,record.commands,required,scope),approval=approvalSignature(app,requirements,record.commands,required);
    const stateField=app.fields.find(f=>f.path===required[0])?.stateField;
    const approvalField=app.fields.find(f=>f.source==='state'&&f.role==='approval-status')?.path;
    if(record.idiom==='scalar-edit'&&(!edit||!idiomChildren(record.idiomNode,stateField,'input')))fail('idiom-signature',scope);
    if(record.idiom==='approval-decision'&&(!approval||!idiomChildren(record.idiomNode,approvalField,'selector')))fail('idiom-signature',scope);
    if(v2&&!record.idiom&&swapFits(app,requirements,record.commands,required))summary.idiomMisfit++;
  }
  if(v2){
    const records=[...scopes.values()],allCommands=app.commands.map(c=>c.id);
    const updateAvailable=allCommands.some(id=>editSignature(app,requirements,[id],scopeInputs(app,requirements,id)??[],id));
    const approvalAvailable=approvalSignature(app,requirements,allCommands,scopeInputs(app,requirements,'selected')??[]);
    if(updateAvailable&&!records.some(r=>r.idiom==='scalar-edit'))summary.idiomMisfit++;
    if(approvalAvailable&&!records.some(r=>r.idiom==='approval-decision'))summary.idiomMisfit++;
  }
  summary.duplicateInputs=[...scopes.values()].reduce((n,r)=>n+r.input.size,0)-new Set([...scopes.values()].flatMap(r=>[...r.input.keys()])).size;
  if(v2){
    // Version 2: who is acting is shown once; every state input is displayed once outside the
    // technical record, whose panels stay collapsed.
    if(summary.acting!==1)fail('acting-coverage','$');
    const state=app.fields.filter(f=>f.source==='state').map(f=>f.path);
    if(state.some(p=>displayed.get(p)!==1)||displayed.size!==state.length)fail('display-coverage','$');
    if(summary.expandedExpertPanels>0)fail('expert-collapsed','$');
  }else if(summary.acting>0||displays>0)fail('node-schema','$');
  if(summary.visibleForms>requirements.maxVisibleForms)fail('max-visible-forms','$');
  if(summary.expandedExpertPanels>requirements.maxExpandedExpertPanels)fail('max-expanded-expert-panels','$');
  if(summary.columns>requirements.maxColumns)fail('max-columns','$');
  return {valid:failures.length===0,failures,summary};
}
// Lower is better, compared in order: idiom fit, then presentation (the state summary before the
// actions, few inputs before each review), then the structural preferences and counts.
export function scoreAst(_app, requirements, _ast, summary) {
  const grouped=summary.scopes.some(s=>s==='selected'||s.startsWith('group-'));
  const mismatches=Number(grouped!==requirements.groupCommands)+Number((summary.expandedExpertPanels===0)!==requirements.collapseExpert)+Number((summary.columns===1)!==requirements.compact);
  return [summary.idiomMisfit,summary.summaryAfterCommands,mismatches,summary.primaryCardGaps,summary.duplicateInputs,summary.visibleSecondary,summary.inputsBeforeReview,summary.expandedExpertPanels,summary.nodes,summary.depth];
}
