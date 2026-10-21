// Shared design-system components, using the demo's native markup and tokens.
export function node(tag, className, ...children) {
  const result=document.createElement(tag); if(className)result.className=className;
  result.append(...children.filter(v=>v!==null&&v!==undefined));return result;
}
export function metrics(...items) {
  const list=node('dl','metrics');
  for(const [value,label,tone] of items)list.append(node('div',tone?`metric ${tone}`:'metric',node('dt',null,label),node('dd',null,value)));
  return list;
}
export function control(field, id, initial) {
  const input=node(field.options.length?'select':'input');input.id=id;
  if(field.options.length)for(const o of field.options){const item=node('option',null,o.label);item.value=o.value;input.append(item);}
  else {input.type='text';input.inputMode='numeric';input.maxLength=40;input.required=true;input.pattern='-?(0|[1-9][0-9]*)';}
  input.value=initial;
  const label=node('label',null,field.label);label.htmlFor=id;
  const domain=field.options.length?field.options.map(o=>o.label).join(', '):`${field.domain.min} to ${field.domain.max}`;
  const help=node('p','help',`${field.type_name ?? field.domain.kind}; ${field.unit}; ${domain}.`);help.id=`${id}-help`;input.setAttribute('aria-describedby',help.id);
  return {input,wrapper:node('div','field',label,input,help)};
}
export const verdict=(message)=>node('p','verdict',message);
export const comparison=(message)=>node('div','decision-comparison',node('p',null,message));
export function ledger(entries) {
  const list=node('ol');for(const e of entries)list.append(node('li',null,`${e.command}: ${e.result}.`));
  return entries.length?list:node('p','help','No commands decided yet.');
}
export function record(value) {return node('details','technical-record',node('summary',null,'Technical record'),node('pre',null,JSON.stringify(value,null,2)));}
