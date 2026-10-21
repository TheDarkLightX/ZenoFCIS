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
function text(value, error) { if (typeof value !== 'string' || !value.trim() || value.length>300 || /[<>\x00-\x1f]/.test(value)) throw Error(error); return value; }
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
    exact(m,['label','unit'],'FieldMetadata');
    const options=f.domain.kind==='bool' ? [{value:'0',label:'No'},{value:'1',label:'Yes'}] : f.domain.kind==='sum'
      ? f.options.map(o=>({value:o.value,label:text(meta.variants[o.value],`UiMissingLabel:variant.${o.value}`)})) : [];
    if (!['bool','sum','int'].includes(f.domain.kind)) throw Error(`UiUnsupportedDomain:${f.path}`);
    const out={...f,label:text(m.label,`UiMissingLabel:${f.path}`),unit:text(m.unit,`UiMissingUnit:${f.path}`),options};
    if (!inDomain(out,out.initial)) throw Error(`UiInvalidInitial:${f.path}`);
    return out;
  });
  if (Object.keys(meta.fields).some(p=>!fields.some(f=>f.path===p))) throw Error('UiUnknownFieldMetadata');
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
  return `${field.options.find(o=>o.value===value)?.label ?? value} ${field.unit}`;
}
