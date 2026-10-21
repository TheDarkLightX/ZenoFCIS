// Data-only descriptors. This module has no DOM, time, randomness or I/O.
export const REGIONS = Object.freeze(['state','commands','confirmation','verdict','metrics','comparison','ledger','record']);
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
function text(value, error) { if (typeof value !== 'string' || !value.trim() || value.length>300 || /[<>\p{Cc}\p{Cf}\p{Cs}\p{Co}\p{Zl}\p{Zp}\u034f\u115f\u1160\u17b4\u17b5\u180b-\u180f\u2065\u3164\ufe00-\ufe0f\uffa0\ufff0-\ufff8\u{E0000}-\u{E0FFF}]/u.test(value)) throw Error(error); return value; }
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
export function describe(raw, meta) {
  exact(meta,['format','title','action','fields','variants','commands','reasons'],'Metadata');
  if (meta.format!=='zenoui/metadata/1') throw Error('UiMetadataVersion');
  text(meta.title,'UiMissingLabel:title');
  if (!Array.isArray(raw.fields) || !raw.fields.length) throw Error('UiMissingFields');
  const fields=raw.fields.map(f=>{
    const m=meta.fields[f.path];
    if (!m?.label) throw Error(`UiMissingLabel:${f.path}`);
    if (!m?.unit) throw Error(`UiMissingUnit:${f.path}`);
    exact(m,Object.hasOwn(m,'values')?['label','unit','values']:['label','unit'],'FieldMetadata');
    const options=f.domain.kind==='bool' ? [{value:'0',label:'No'},{value:'1',label:'Yes'}] : f.domain.kind==='sum'
      ? f.options.map(o=>({value:o.value,label:text(meta.variants[o.value],`UiMissingLabel:variant.${o.value}`)})) : [];
    if (!['bool','sum','int'].includes(f.domain.kind)) throw Error(`UiUnsupportedDomain:${f.path}`);
    if (Object.hasOwn(f,'clock') && (f.clock!==true || f.source!=='context' || f.domain.kind!=='int')) throw Error(`UiInvalidClock:${f.path}`);
    const out={...f,label:text(m.label,`UiMissingLabel:${f.path}`),unit:text(m.unit,`UiMissingUnit:${f.path}`),options};
    if (!inDomain(out,out.initial)) throw Error(`UiInvalidInitial:${f.path}`);
    if (Object.hasOwn(m,'values')) out.values=valueLabels(out,m.values);
    return out;
  });
  if (Object.keys(meta.fields).some(p=>!fields.some(f=>f.path===p))) throw Error('UiUnknownFieldMetadata');
  if (fields.filter(f=>f.clock).length>1) throw Error('UiInvalidClock');
  const action=fields.find(f=>f.path===meta.action && f.source==='command' && f.domain.kind==='sum');
  if (!action) throw Error('UiMissingCommandDiscriminator');
  const commands=action.options.map(o=>{
    const m=meta.commands[o.value];
    if (!m?.label) throw Error(`UiMissingLabel:command.${o.value}`);
    exact(m,['label','consequential'],'CommandMetadata');
    if(typeof m.consequential!=='boolean') throw Error(`UiMissingConsequence:${o.value}`);
    return {id:o.value,label:text(m.label,`UiMissingLabel:command.${o.value}`),consequential:m.consequential};
  });
  if (Object.keys(meta.commands).some(k=>!commands.some(c=>c.id===k))) throw Error('UiUnknownCommandMetadata');
  const reasons=raw.reasons.map(r=>({id:r.id,message:text(meta.reasons[r.id],`UiMissingReasonMessage:${r.id}`)}));
  if (Object.keys(meta.reasons).some(k=>!reasons.some(r=>r.id===k))) throw Error('UiUnknownReasonMetadata');
  if(canonical(raw.enablement?.table)!=='[false,false,false,true]')throw Error('UiEnablementJudgeRefused');
  return freeze({format:'zenoui/app-description/1',title:meta.title,action:action.index,fields,commands,reasons,domain:raw.domain,enablement:raw.enablement.table,layout:REGIONS.map((id,slot)=>({id,slot}))});
}
export function layoutCheck(layout) {
  if (!Array.isArray(layout)||layout.length!==REGIONS.length||new Set(layout.map(r=>r.id)).size!==REGIONS.length||new Set(layout.map(r=>r.slot)).size!==REGIONS.length) throw Error('L4_layout_declared_once');
  for(const r of layout) if(!REGIONS.includes(r.id)||!Number.isInteger(r.slot)||r.slot<0||r.slot>=REGIONS.length||layout.find(x=>x.slot===r.slot)?.id!==r.id)throw Error('L4_layout_declared_once');
  return true;
}
export function display(field,value) {
  const named=field.options.find(o=>o.value===value)?.label ?? (field.values && Object.hasOwn(field.values,value) ? field.values[value] : null);
  // A named value reads alone; "unitless" is never shown.
  return named ?? (field.unit==='unitless' ? value : `${value} ${field.unit}`);
}
