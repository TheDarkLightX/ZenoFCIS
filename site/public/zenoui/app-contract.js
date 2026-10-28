import {synthesizePresentation,verifyPresentation as verifyTree,checkPresentation as checkTree,deriveRequirements} from './layout-search.js';
import {INPUT_ROLES} from './ui-idioms.js';
// Data-only descriptors. This module has no DOM, time, randomness or I/O.
export const REGIONS = Object.freeze(['state','commands','confirmation','verdict','metrics','comparison','ledger','record']);
// Field roles and page archetypes: ui.json may name one; otherwise a stated rule picks it.
export const ROLES = Object.freeze(['stage','status','amount','flag','identity','approval-status',...INPUT_ROLES]);
export const ARCHETYPES = Object.freeze(['workflow','balances','lanes']);
// Traverse nested values even if the outer object was already frozen.
export function freeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); }
  return value;
}
export function canonical(value) {
  return JSON.stringify(value,(_key,node)=>node && typeof node==='object' && !Array.isArray(node)
    ? Object.fromEntries(Object.keys(node).sort().map(k=>[k,node[k]])) : node);
}

function exact(value, keys, name) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).some(k=>!keys.includes(k)) || keys.some(k=>!Object.hasOwn(value,k))) throw Error(`UiInvalid${name}`);
}
const optional=(value,keys,extra)=>[...keys,...extra.filter(k=>Object.hasOwn(value??{},k))];
function text(value, error) { if (typeof value !== 'string' || !value.trim() || value.length>300 || /[<>\p{Cc}\p{Cf}\p{Cs}\p{Co}\p{Zl}\p{Zp}\u034f\u115f\u1160\u17b4\u17b5\u180b-\u180f\u2065\u3164\ufe00-\ufe0f\uffa0\ufff0-\ufff8\u{E0000}-\u{E0FFF}]/u.test(value)) throw Error(error); return value; }
// Versioned grouping of existing scalar state positions, never a query or new
// storage model. Native capture and factory source/asset bindings establish the
// origin of this data; this pure check verifies its complete coordinate binding.
function fixedTables(fields, native, metadata) {
  exact(native,['format','tables'],'NativeFixedTables');
  if(native.format!=='zenoui/native-fixed-tables/1')throw Error('UiNativeFixedTablesVersion');
  if(!Array.isArray(native.tables)||!native.tables.length||native.tables.length>64)throw Error('UiNativeFixedTablesRequired');
  const id=value=>typeof value==='string'&&/^[1-9][0-9]{0,9}$/.test(value)&&BigInt(value)<=4294967295n;
  const ids=native.tables.map(t=>t?.id);
  exact(metadata,ids,'FixedTableMetadata');
  const used=new Set(),seen=new Set();let previous=0n;
  return native.tables.map(table=>{
    exact(table,['id','owner','name','fieldType','size','entries'],'NativeFixedTable');
    if(!id(table.id)||table.owner!=='100'||!id(table.fieldType)||BigInt(table.id)<=previous||seen.has(table.id))throw Error('UiFixedTableIdentity');
    previous=BigInt(table.id);seen.add(table.id);text(table.name,'UiFixedTableName');
    if(!Number.isSafeInteger(table.size)||table.size<1||table.size>64||!Array.isArray(table.entries)||table.entries.length!==table.size)throw Error('UiFixedTableSize');
    const m=metadata[table.id];exact(m,['label'],'FixedTableMetadata');text(m.label,'UiFixedTableLabel');
    let unit,decimals;
    const entries=table.entries.map((entry,offset)=>{
      exact(entry,['field','path','index'],'NativeTableEntry');
      const expected=(BigInt(table.id)+BigInt(offset)).toString();
      if(!id(entry.field)||entry.field!==expected||BigInt(entry.field)>65535n||entry.path!==`pre.100.${entry.field}`||
         !Number.isSafeInteger(entry.index)||entry.index<0||entry.index>=fields.length||used.has(entry.path))throw Error('UiFixedTableEntry');
      const f=fields[entry.index];
      if(f.index!==entry.index||f.source!=='state'||f.path!==entry.path||!Number.isSafeInteger(f.field)||!Number.isSafeInteger(f.type)||String(f.field)!==entry.field||String(f.type)!==table.fieldType||fields.filter(x=>x.path===entry.path).length!==1)throw Error('UiFixedTableFieldBinding');
      if(offset===0){unit=f.unit;decimals=f.decimals??0;}
      else if(f.unit!==unit||(f.decimals??0)!==decimals)throw Error('UiFixedTableUnits');
      used.add(entry.path);return entry.path;
    });
    return {id:table.id,label:m.label,entries};
  });
}
// Optional labels for chosen integer values, such as 0 for "on shelf".
function valueLabels(field, values) {
  const keys=values && typeof values==='object' && !Array.isArray(values) ? Object.keys(values) : [];
  if (field.domain.kind!=='int' || !keys.length || keys.length>64 || keys.some(k=>!inDomain(field,k))) throw Error(`UiInvalidValueLabels:${field.path}`);
  return Object.fromEntries(keys.map(k=>[k,text(values[k],`UiMissingLabel:${field.path}.${k}`)]));
}
export function inDomain(field, value) {
  if (typeof value !== 'string' || value.length>40 || !/^(0|-?[1-9][0-9]*)$/.test(value)) return false;
  if (field.domain.kind === 'int') return BigInt(value)>=BigInt(field.domain.min) && BigInt(value)<=BigInt(field.domain.max);
  return field.options.some(o=>o.value===value);
}
// Levels of an acyclic stage graph: each value sits at the longest path from a start; null when cyclic.
function levels(field, pairs) {
  const values=field.options.map(o=>o.value),level=Object.fromEntries(values.map(v=>[v,0])),incoming=Object.fromEntries(values.map(v=>[v,0]));
  for (const [,to] of pairs) incoming[to]++;
  const ready=values.filter(v=>!incoming[v]),seen=[];
  while (ready.length) { const v=ready.shift();seen.push(v);
    for (const [from,to] of pairs) if (from===v) { level[to]=Math.max(level[to],level[v]+1); if (--incoming[to]===0) ready.push(to); } }
  if (seen.length!==values.length) return null;
  const out=[];for (const v of values) (out[level[v]]??=[]).push(v);
  return out.filter(Boolean);
}
function role(field, m, s) {
  const named=field.domain.kind==='sum'||(field.domain.kind==='int'&&field.values);
  if (Object.hasOwn(m,'role')) {
    const input=field.source==='command'&&field.domain.kind==='int';
    const r=m.role,fits={stage:named,status:named,amount:field.domain.kind==='int',flag:field.domain.kind==='bool',identity:field.domain.kind!=='bool','amount-in':input,'min-out':input,'requested-value':input,
      'approval-status':field.source==='state'&&(field.domain.kind==='bool'||named)};
    if (!ROLES.includes(r) || !fits[r] || (r==='stage'&&!(field.source==='state'&&field.domain.kind==='sum'))) throw Error(`UiInvalidRole:${field.path}`);
    return r;
  }
  if (field.domain.kind==='bool') return 'flag';
  if (field.domain.kind==='sum') { const pairs=s?.transitions[String(field.index)];
    return field.source==='state' && pairs?.length && levels(field,pairs) ? 'stage' : 'status'; }
  return field.unit!=='unitless' ? 'amount' : field.values ? 'status' : 'identity';
}
const words=label=>label.split(' ');
function prefix(a, b) { const x=words(a),y=words(b),out=[];for (let i=0;i<Math.min(x.length,y.length)&&x[i]===y[i];i++) out.push(x[i]);return out; }
// Lanes: repeated status fields of one type. A lane takes the following fields whose labels
// share its leading words, such as "Lane A status" and "Lane A reserved".
function lanes(fields) {
  const state=fields.filter(f=>f.source==='state'),status=state.filter(f=>f.role==='status');
  const repeated=status.filter(f=>status.filter(g=>g.type_name===f.type_name).length>1);
  return repeated.map((lane,k)=>{
    const end=repeated[k+1]?.index??Infinity,members=[];let title=lane.label;
    for (const f of state.filter(f=>f.index>lane.index&&f.index<end)) {
      const shared=prefix(lane.label,f.label);
      if (!shared.length || shared.length>=words(lane.label).length || shared.length>=words(f.label).length) break;
      title=shared.join(' ');const rest=f.label.slice(title.length+1);members.push({index:f.index,label:rest[0].toUpperCase()+rest.slice(1)});
    }
    return {title,lane:lane.index,members};
  });
}
export function describe(raw, meta) {
  const tableProfile=meta?.format==='zenoui/metadata/3',rich=meta?.format==='zenoui/metadata/2'||tableProfile;
  exact(meta,rich?optional(meta,['format','title','action','fields','variants','commands','reasons',...(tableProfile?['tables']:[])],['archetype','requirements']):['format','title','action','fields','variants','commands','reasons'],'Metadata');
  if (!['zenoui/metadata/1','zenoui/metadata/2','zenoui/metadata/3'].includes(meta.format)) throw Error('UiMetadataVersion');
  if (rich && !Object.hasOwn(meta,'requirements')) throw Error('UiInvalidMetadata');
  // Only requirements/2 separates server-supplied context from editable forms.
  if (rich && meta.requirements?.format!=='zenoui/requirements/2') throw Error('UiUnsupportedRequirementsVersion');
  text(meta.title,'UiMissingLabel:title');
  if (!Array.isArray(raw.fields) || !raw.fields.length) throw Error('UiMissingFields');
  const fields=raw.fields.map(f=>{
    const m=meta.fields[f.path];
    if (!m?.label) throw Error(`UiMissingLabel:${f.path}`);
    if (!m?.unit) throw Error(`UiMissingUnit:${f.path}`);
    exact(m,optional(m,['label','unit'],rich?['values','role','decimals','stateField']:['values']),'FieldMetadata');
    const options=f.domain.kind==='bool' ? [{value:'0',label:'No'},{value:'1',label:'Yes'}] : f.domain.kind==='sum'
      ? f.options.map(o=>({value:o.value,label:text(meta.variants[o.value],`UiMissingLabel:variant.${o.value}`)})) : [];
    if (!['bool','sum','int'].includes(f.domain.kind)) throw Error(`UiUnsupportedDomain:${f.path}`);
    if (Object.hasOwn(f,'clock') && (f.clock!==true || f.source!=='context' || f.domain.kind!=='int')) throw Error(`UiInvalidClock:${f.path}`);
    const out={...f,label:text(m.label,`UiMissingLabel:${f.path}`),unit:text(m.unit,`UiMissingUnit:${f.path}`),options};
    if (!inDomain(out,out.initial)) throw Error(`UiInvalidInitial:${f.path}`);
    if (Object.hasOwn(m,'values')) out.values=valueLabels(out,m.values);
    // Decimal places of an integer amount held in its smallest unit, such as 18 for a token.
    if (Object.hasOwn(m,'decimals')) { if (f.domain.kind!=='int'||!Number.isInteger(m.decimals)||m.decimals<1||m.decimals>38) throw Error(`UiInvalidDecimals:${f.path}`); out.decimals=m.decimals; }
    return out;
  });
  if (Object.keys(meta.fields).some(p=>!fields.some(f=>f.path===p))) throw Error('UiUnknownFieldMetadata');
  if (fields.filter(f=>f.clock).length>1) throw Error('UiInvalidClock');
  if(Object.hasOwn(raw,'tables')||Object.hasOwn(raw,'semantics'))throw Error('UiUnsupportedUnboundSemanticsOrTables');
  const action=fields.find(f=>f.path===meta.action && f.source==='command' && f.domain.kind==='sum');
  if (!action) throw Error('UiMissingCommandDiscriminator');
  const commands=action.options.map(o=>{
    const m=meta.commands[o.value];
    if (!m?.label) throw Error(`UiMissingLabel:command.${o.value}`);
    exact(m,optional(m,['label','consequential'],rich?['uiRole']:[]),'CommandMetadata');
    if(typeof m.consequential!=='boolean') throw Error(`UiMissingConsequence:${o.value}`);
    if(Object.hasOwn(m,'uiRole')&&(!['update','approve','reject'].includes(m.uiRole)||!m.consequential)) throw Error(`UiInvalidCommandRole:${o.value}`);
    return {id:o.value,label:text(m.label,`UiMissingLabel:command.${o.value}`),consequential:m.consequential,...(Object.hasOwn(m,'uiRole')?{uiRole:m.uiRole}:{})};
  });
  if (Object.keys(meta.commands).some(k=>!commands.some(c=>c.id===k))) throw Error('UiUnknownCommandMetadata');
  // A reason is a message, or a message plus the text shown when no input of a command can pass in the
  // current state, which must not ask for a different input.
  const reasons=raw.reasons.map(r=>{const v=meta.reasons[r.id],missing=`UiMissingReasonMessage:${r.id}`;
    if (meta.format==='zenoui/metadata/1' || typeof v!=='object' || v===null) return {id:r.id,message:text(v,missing)};
    exact(v,optional(v,['message'],['unavailable']),'ReasonMetadata');
    return {id:r.id,message:text(v.message,missing),...(Object.hasOwn(v,'unavailable')?{unavailable:text(v.unavailable,missing)}:{})};});
  if (Object.keys(meta.reasons).some(k=>!reasons.some(r=>r.id===k))) throw Error('UiUnknownReasonMetadata');
  if(canonical(raw.enablement?.table)!=='[false,false,false,true]')throw Error('UiEnablementJudgeRefused');
  // Without the checked model every command input is shown and every command is offered for preview.
  const s=null;
  for (const f of fields) { f.role=role(f,meta.fields[f.path],s);
    if (f.role==='stage') f.stages=levels(f,s?.transitions[String(f.index)]??[]) ?? [f.options.map(o=>o.value)]; }
  for (const f of fields) {
    const m=meta.fields[f.path];
    if(f.role==='requested-value'){
      const current=fields.find(x=>x.path===m.stateField);
      if(typeof m.stateField!=='string'||current?.source!=='state'||current.domain.kind!=='int'||
         current.unit!==f.unit||(current.decimals??0)!==(f.decimals??0))throw Error(`UiInvalidStateField:${f.path}`);
      f.stateField=m.stateField;
    }else if(Object.hasOwn(m,'stateField'))throw Error(`UiInvalidStateField:${f.path}`);
  }
  const tables=tableProfile?fixedTables(fields,raw.nativeTables,meta.tables):null;
  const tableEntries=new Set((tables??[]).flatMap(t=>t.entries));
  const groups=lanes(fields.filter(f=>!tableEntries.has(f.path)));
  if (Object.hasOwn(meta,'archetype') && (!ARCHETYPES.includes(meta.archetype) || (meta.archetype==='lanes'&&!groups.length) || (meta.archetype==='workflow'&&!fields.some(f=>f.role==='stage'))))
    throw Error('UiInvalidArchetype');
  const archetype=meta.archetype ?? (fields.some(f=>f.role==='stage') ? 'workflow' : groups.length ? 'lanes' : 'balances');
  const inputs=fields.filter(f=>f.source!=='state'&&f.index!==action.index).map(f=>f.index);
  const relevant=Object.fromEntries(commands.map(c=>[c.id,s?s.relevant[c.id]:inputs]));
  const app={format:tableProfile?'zenoui/app-description/3':'zenoui/app-description/2',title:meta.title,action:action.index,fields,commands,reasons,domain:raw.domain,enablement:raw.enablement.table,
    archetype,lanes:archetype==='lanes'?groups:[],relevant,situation:s?.situation??null,possible:s?.possible??null,bounded:!!s?.bounded,layout:REGIONS.map((id,slot)=>({id,slot}))};
  if(tableProfile)Object.assign(app,{nativeTables:structuredClone(raw.nativeTables),tables});
  // The UI synthesizer searches the component grammar for every app: ui.json may state the
  // requirements (metadata/2); otherwise they follow conservatively from the descriptor.
  const result=synthesizePresentation(app,rich?meta.requirements:deriveRequirements(app));
  Object.assign(app,result,{layout:result.presentation.layout});
  return freeze(app);
}
// No native relevance/possibility evidence is currently available here.
export function possibleAt() { return null; }
export function preferredContext() { return null; }
// The command inputs a form shows. Context is handled separately.
export function formFields(app, command) {
  return app.fields.filter(f=>f.source==='command'&&f.index!==app.action&&app.relevant[command].includes(f.index));
}
export function layoutCheck(layout) {
  if (!Array.isArray(layout)||layout.length!==REGIONS.length||new Set(layout.map(r=>r.id)).size!==REGIONS.length||new Set(layout.map(r=>r.slot)).size!==REGIONS.length) throw Error('L4_layout_declared_once');
  for(const r of layout) if(!REGIONS.includes(r.id)||!Number.isInteger(r.slot)||r.slot<0||r.slot>=REGIONS.length||layout.find(x=>x.slot===r.slot)?.id!==r.id)throw Error('L4_layout_declared_once');
  return true;
}
// An amount's exact decimal text: its smallest units shifted by the field's decimal places, with
// thousands separators and no trailing zeros. String and BigInt arithmetic only; nothing is rounded.
export function amountText(field,value) {
  if (isInvalidAmount(value)) return value.invalidAmount;
  if (!field.decimals || typeof value!=='string' || !/^-?(0|[1-9][0-9]*)$/.test(value)) return value;
  const negative=value.startsWith('-'),digits=(negative?value.slice(1):value).padStart(field.decimals+1,'0');
  const whole=digits.slice(0,-field.decimals).replace(/\B(?=(\d{3})+(?!\d))/g,','),part=digits.slice(-field.decimals).replace(/0+$/,'');
  return `${negative?'-':''}${whole}${part?`.${part}`:''}`;
}
// Entered decimal amounts follow the decimal input rules (site/tests/zenoui-decimal-rules.test.mjs):
// exact BigInt base units; more fraction digits than the scale refused, never rounded; one plain
// spelling (ASCII digits, at most one '.', '-' only first, no grouping, exponent, '+' or space); at
// most 41 characters; and the declared range. Grouping is for display only, never for editable text.
const DECIMAL=/^-?(0|[1-9][0-9]*)(\.[0-9]+)?$/;
export function parseDecimal(text,{scale,min,max}) {
  if (typeof text!=='string') return {ok:false,error:'DecimalSyntax'};
  if (text.length>41) return {ok:false,error:'DecimalLength'};
  if (!DECIMAL.test(text)||/^-0(\.0+)?$/.test(text)) return {ok:false,error:'DecimalSyntax'};
  const negative=text.startsWith('-'),[whole,fraction='']=text.slice(negative?1:0).split('.');
  if (fraction.length>scale) return {ok:false,error:'DecimalScale'};
  const magnitude=BigInt(whole+fraction.padEnd(scale,'0')),units=negative?-magnitude:magnitude;
  if (units<BigInt(min)||units>BigInt(max)) return {ok:false,error:'DecimalRange'};
  return {ok:true,units:units.toString()};
}
// The editable text of an amount: its plain decimal form, without grouping or trailing zeros.
export function formatDecimal(units,scale) {
  const value=BigInt(units),negative=value<0n,digits=(negative?-value:value).toString().padStart(scale+1,'0');
  const whole=digits.slice(0,digits.length-scale),fraction=scale?digits.slice(digits.length-scale).replace(/0+$/,''):'';
  return `${negative?'-':''}${whole}${fraction?`.${fraction}`:''}`;
}
// Invalid decimal drafts retain text for correction. They never satisfy inDomain or reach transport.
export function isInvalidAmount(value) {
  return value!==null && typeof value==='object' && !Array.isArray(value) && Object.keys(value).length===1 && typeof value.invalidAmount==='string';
}
export function editText(field,value) {
  if (isInvalidAmount(value)) return value.invalidAmount;
  return field.decimals && typeof value==='string' && /^-?(0|[1-9][0-9]*)$/.test(value) ? formatDecimal(value,field.decimals) : value;
}
// The smallest units an entered decimal amount names, or null when the text is not an exact amount.
export function parseAmount(field,text) {
  if (!field.decimals) return null;
  const result=parseDecimal(text,{scale:field.decimals,min:field.domain.min,max:field.domain.max});
  return result.ok?result.units:null;
}
// The input a command's request names as its minimum to receive (ui.json role min-out), if it decides.
export function minOutField(app,id) {
  return app.fields.find(f=>f.role==='min-out'&&f.source==='command'&&(app.relevant?.[id]??[]).includes(f.index))??null;
}
export function display(field,value) {
  const named=field.options.find(o=>o.value===value)?.label ?? (field.values && Object.hasOwn(field.values,value) ? field.values[value] : null);
  // A named value reads alone; "unitless" is never shown.
  return named ?? (field.unit==='unitless' ? amountText(field,value) : `${amountText(field,value)} ${field.unit}`);
}

// Refuse a forged or stale narrowed-relevance descriptor at generation and mount.
function checkFallback(app) {
  const tableProfile=app.format==='zenoui/app-description/3';
  if(!['zenoui/app-description/2','zenoui/app-description/3'].includes(app.format)||app.possible!==null||app.situation!==null||app.bounded!==false||
     (!tableProfile&&(Object.hasOwn(app,'tables')||Object.hasOwn(app,'nativeTables'))))throw Error('UiUnsupportedUnboundSemanticsOrTables');
  if(tableProfile){
    if(!Array.isArray(app.tables))throw Error('UiFixedTableBindingsRequired');
    const metadata=Object.fromEntries(app.tables.map(t=>[t?.id,{label:t?.label}]));
    const expected=fixedTables(app.fields,app.nativeTables,metadata);
    if(canonical(expected)!==canonical(app.tables))throw Error('UiFixedTableBindingsChanged');
  }
  if(app.presentation?.requirements?.format!=='zenoui/requirements/2')throw Error('UiUnsupportedRequirementsVersion');
  const inputs=app.fields.filter(f=>f.source!=='state'&&f.index!==app.action).map(f=>f.index);
  if(!app.relevant||canonical(Object.keys(app.relevant).sort())!==canonical(app.commands.map(c=>c.id).sort()))throw Error('UiInvalidFallbackRelevance');
  for(const c of app.commands)if(canonical(app.relevant[c.id])!==canonical(inputs))throw Error('UiInvalidFallbackRelevance');
}
export function checkPresentation(app) { checkFallback(app); return checkTree(app); }
export function verifyPresentation(app) { checkFallback(app); return verifyTree(app); }
