// The thin render step: one element per node of the checked component tree, keeping the tree's
// topology, with each node's view (from ui-view.js) mapped onto it. It computes nothing.
import {node,button,badge,keyValues,record,notice} from './components.js';
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
function labelled(id, label, control, help, suffix) {
  const l=node('label','zu-label',label);l.htmlFor=id;
  const h=help?node('p','zu-help',help):null;if(h){h.id=`${id}-help`;control.setAttribute('aria-describedby',h.id);}
  return node('div','zu-field',l,suffix?node('div','zu-input-row',control,node('span','zu-suffix',suffix)):control,h);
}
function control(view) {
  const input=node(view.choices?'select':'input','zu-input');input.id=view.id;
  if(view.choices)for(const o of view.choices){const item=node('option',null,o.label);item.value=o.value;input.append(item);}
  else{input.type='text';input.inputMode=view.decimal?'decimal':'numeric';input.autocomplete='off';input.maxLength=view.decimal?41:40;input.required=true;input.pattern=view.decimal?'-?(0|[1-9][0-9]*)([.][0-9]+)?':'-?(0|[1-9][0-9]*)';}
  return input;
}
function history(entries, empty) {
  if(!entries.length)return node('p','zu-empty',empty);
  return node('ol','zu-timeline',...entries.map(e=>node('li','zu-timeline-item',node('span','zu-timeline-step',e.step),node('span','zu-timeline-text',e.command),badge(e.result,e.tone))));
}
function displayContent(idiom, s) {
  if(idiom==='table-list'){
    if(!s.entries.length)return [];
    const list=node('ul','zu-table-list',...s.entries.map(e=>{const item=node('li','zu-table-entry',node('span','zu-table-label',e.label),node('span','zu-value',e.value));item.dataset.field=e.index;return item;}));
    list.setAttribute('aria-label',s.label);return [node('h3',null,s.label),list];
  }
  if(idiom==='stepper'){
    if(s.empty)return [];
    const list=node('ol','zu-stepper');list.setAttribute('aria-label',s.label);
    s.steps.forEach((step,i)=>{const li=node('li',`zu-step is-${step.state}`,node('span','zu-step-mark',String(i+1)),node('span','zu-step-label',step.label),node('span','zu-visually-hidden',step.state==='done'?' (passed)':step.state==='current'?' (current)':''));if(step.state==='current')li.setAttribute('aria-current','step');list.append(li);});
    const box=node('div','zu-stage',node('p','zu-stage-label',s.label),list);box.dataset.field=s.index;return [box];
  }
  if(idiom==='metrics'){
    const list=node('dl','zu-metrics');
    for(const m of s.items){const value=m.named||!m.unit||m.unit==='unitless'?node('span','zu-value',m.value):node('span','zu-value',node('span','zu-number',m.raw),' ',node('span','zu-unit',m.unit));
      const card=node('div','zu-metric',node('dt',null,m.label),node('dd',null,value));card.dataset.field=m.index;list.append(card);}
    return [list];
  }
  if(idiom==='lanes')return [node('div','zu-lanes',...s.lanes.map(l=>{const card=node('article','zu-lane',node('div','zu-lane-head',node('h3',null,l.title),badge(l.status,'info')),l.rows.length?keyValues(l.rows.map(r=>({label:r.label,content:r.value,index:r.index}))):null);card.dataset.field=l.index;return card;}))];
  return [keyValues(s.rows.map(r=>({label:r.label,index:r.index,content:r.badge?badge(r.value,r.tone):node('span','zu-value',r.value)})))];
}
function evidenceContent(region, s) {
  if(region==='state')return [node('h3',null,s.heading),keyValues(s.rows.map(r=>({label:r.label,content:r.value})))];
  if(region==='metrics'||region==='comparison')return [node('p','zu-meta',s.text)];
  if(region==='ledger')return [node('h3',null,s.heading),history(s.entries,s.empty)];
  return [record(s.record)];
}
// Returns the page element and an update function that maps a whole view tree onto it.
export function buildTree(ast, initial, handlers) {
  const updaters=[],on=fn=>updaters.push(fn);
  let build=function(ast,path,context) {
    const v=initial.nodes[path]??{};
    const kids=(el,ctx=context)=>{ast.children.forEach((child,i)=>el.append(build(child,`${path}.${i}`,ctx)));return el;};
    switch(ast.kind){
      case 'page':return kids(node('div','zu-page'));
      case 'stack':return kids(node('div','zu-stack'));
      case 'card':return kids(node('div','zu-card'));
      case 'grid':return kids(node('div','zu-grid'));
      case 'disclosure':return kids(node('details',v.label==='Technical record'?'zu-disclosure zu-technical':'zu-disclosure',node('summary',null,v.label)));
      case 'commands':{
        const s=node('section','zu-region zu-commands'),empty=node('p','zu-empty',v.emptyText),again=button('Check again','ghost'),jump=node('a','zu-next-action');s.dataset.region='commands';
        let next=null;
        again.addEventListener('click',handlers.refresh);
        jump.addEventListener('click',event=>{event.preventDefault();const target=next;if(!target)return;if(target.select)handlers.select(target.command);document.getElementById(`action-${target.scope}-title`)?.focus();});
        s.append(node('div','zu-section-head',node('h2',null,v.heading),again),jump,empty);
        on(n=>{const current=n.nodes[path];next=current.next;empty.hidden=!current.empty;empty.textContent=current.emptyText;again.disabled=n.phase==='running';
          jump.hidden=!next;if(next){jump.textContent=`Go to ${next.label}`;jump.href=`#action-${next.scope}-title`;}});return kids(s);
      }
      case 'scope':{
        const form=node('form','zu-action'),title=node('h3','zu-action-title',v.label),reasonText=node('span',null),reason=node('p','zu-reason',badge('Not available now','neutral'),' ',reasonText);form.noValidate=true;
        title.id=`action-${ast.scope}-title`;title.tabIndex=-1;form.setAttribute('aria-labelledby',title.id);form.dataset.scope=ast.scope;
        form.addEventListener('submit',event=>{event.preventDefault();handlers.review(ast.scope);});
        const ready=node('p','zu-action-kicker','Ready to review'),preview=node('div','zu-action-preview');
        form.append(ready,title,reason);const body=node('div','zu-action-body');form.append(body);kids(body,{...context,scope:ast.scope});form.append(preview);
        on(n=>{const s=n.nodes[path];title.textContent=s.label;form.className=`zu-action${s.available?'':' is-unavailable'}${s.primary?' is-primary':''}`;form.dataset.ready=String(s.ready);ready.hidden=!s.ready;
          reason.hidden=s.available;reasonText.textContent=s.reason??'';form.dataset.command=s.command;body.hidden=!s.available&&!body.querySelector('.zu-swap')&&!body.querySelector('.zu-workflow-idiom');
          preview.hidden=!s.preview;preview.replaceChildren();if(s.preview){preview.append(node('p','zu-preview-label','Visible preview'),node('p',null,s.preview.changes.length?s.preview.changes.map(c=>`${c.label} → ${c.after}`).join('; '):'No visible state changes in this preview.'));}});
        return form;
      }
      case 'idiom':{
        if(ast.idiom==='scalar-edit'||ast.idiom==='approval-decision'){
          const card=node('div',`zu-workflow-idiom zu-${ast.idiom}`);
          card.dataset.idiom=ast.idiom;
          kids(card,{...context,idiom:ast.idiom});
          return card;
        }
        // Swap card details: the exact rate and price impact of the core's quote, the minimum the
        // request will carry, and the maximum slippage that fills it.
        const card=node('div','zu-swap'),built=ast.children.map((child,i)=>build(child,`${path}.${i}`,{...context,idiom:ast.idiom}));
        const cell=()=>node('dd',null),rate=cell(),impact=cell(),minimum=cell(),warning=node('p','zu-warning');
        const chips=node('div','zu-chips');chips.setAttribute('role','group');chips.setAttribute('aria-label','Max slippage');
        const chip=v.slippage?.options.map(o=>{const b=button(`${o}%`,'chip');b.addEventListener('click',()=>handlers.slippage(o));chips.append(b);return [o,b];})??[];
        const row=(label,value)=>node('div','zu-kv-row',node('dt',null,label),value);
        const info=node('dl','zu-kv zu-swap-info',row('Rate',rate),row('Price impact',impact),row('Minimum received',minimum),row('Max slippage',node('dd',null,chips)));
        card.append(...built.slice(0,-1),info,warning,built.at(-1));
        on(n=>{const s=n.nodes[path];rate.textContent=s.rate??'—';impact.textContent=s.impact?`${s.impact.percent}%`:'—';
          minimum.textContent=s.minimum?.value?`${s.minimum.text} ${s.minimum.unit}`:'—';warning.hidden=!s.impact?.high;
          warning.textContent=s.impact?.high?`High price impact: ${s.impact.percent}%. You would receive much less than the current price suggests.`:'';
          for(const [o,b] of chip){b.setAttribute('aria-pressed',String(s.slippage?.value===o));b.className=`zu-button zu-chip${s.slippage?.value===o?' is-selected':''}`;}});
        return card;
      }
      case 'input':{
        const input=control(v),index=Number(v.id.split('-field-')[1]);
        let owner=v.command;
        // A long amount shrinks with its field instead of being cut off, as it is typed and when shown.
        const size=()=>{input.dataset.size=input.value.length>20?'s':input.value.length>14?'m':input.value.length>10?'l':'xl';};
        input.addEventListener('input',()=>{size();handlers.edit(context.scope,index,input.value);});
        // Keep focused typing for this command; a shared field's new owner restores its own draft.
        on(n=>{const s=n.nodes[path];if(input.value!==s.value&&(s.command!==owner||document.activeElement!==input))input.value=s.value;owner=s.command;input.disabled=s.disabled;size();});
        return labelled(v.id,v.label,input,v.help,v.unit);
      }
      case 'selector':{
        if(context.idiom==='approval-decision'){
          const options=node('div','zu-decision-options');options.setAttribute('role','group');options.setAttribute('aria-label',v.label);
          const choices=v.options.map(option=>{const b=button(option.label,'secondary');b.className='zu-button zu-decision-option';
            b.dataset.decision=option.value;b.addEventListener('click',()=>handlers.select(option.value));options.append(b);return [option.value,b];});
          on(n=>{const s=n.nodes[path];for(const [id,b] of choices){b.disabled=s.disabled;b.setAttribute('aria-pressed',String(s.value===id));}});
          return options;
        }
        if(context.idiom==='swap-card'){
          const flip=button('Switch direction','secondary');flip.className='zu-button zu-button--secondary zu-flip';
          flip.addEventListener('click',()=>handlers.select(flip.dataset.next));
          on(n=>{const s=n.nodes[path],next=s.options.find(o=>o.value!==s.value)??s.options[0];flip.dataset.next=next.value;flip.disabled=s.disabled;flip.setAttribute('aria-label',`Switch to ${next.label}`);});
          return node('div','zu-flip-row',flip);
        }
        const select=node('select','zu-input');select.id=v.id;
        for(const o of v.options){const item=node('option',null,o.label);item.value=o.value;select.append(item);}
        select.addEventListener('change',()=>handlers.select(select.value));
        on(n=>{const s=n.nodes[path];select.value=s.value;select.disabled=s.disabled;});
        return labelled(v.id,v.label,select);
      }
      case 'projection':{
        const value=node('output','zu-projection-value'),unit=node('span','zu-unit');
        // A long exact amount steps down in size instead of being rounded.
        on(n=>{const s=n.nodes[path],text=s.value??'—';value.textContent=text;unit.textContent=s.unit;value.dataset.pending=String(s.pending);value.dataset.size=text.length>20?'s':text.length>14?'m':text.length>10?'l':'xl';});
        return node('div','zu-projection',node('span','zu-projection-label',v.label),node('div','zu-projection-row',value,unit));
      }
      case 'review':{
        const b=button(v.text,'secondary'),feedback=node('p','zu-feedback');b.type='submit';feedback.id=`reason-${context.scope}`;b.setAttribute('aria-describedby',feedback.id);
        on(n=>{const s=n.nodes[path];b.textContent=s.text;b.setAttribute('aria-label',s.aria);b.disabled=!s.enabled;b.dataset.command=s.command;b.hidden=s.hidden;
          b.className=`zu-button zu-button--${s.primary?'primary':'secondary'}${s.block?' zu-button--block':''}`;feedback.textContent=s.feedback;feedback.className=`zu-feedback is-${s.tone}`;});
        return node('div','zu-action-foot',b,feedback);
      }
      case 'confirmation':{
        const section=node('section','zu-region zu-region--confirmation'),confirm=button('','primary'),back=button('Back','secondary');let last;
        section.dataset.region='confirmation';confirm.addEventListener('click',handlers.confirm);back.addEventListener('click',handlers.refresh);
        on(n=>{const c=n.nodes[path];if(same(c,last))return;const opened=!last?.open&&c.open;last=c;
          if(!c.open){section.replaceChildren();return;}
          const heading=node('h2','zu-confirm-title',c.heading);heading.tabIndex=-1;confirm.textContent=c.confirmLabel;
          section.replaceChildren(...[heading,node('p','zu-consequence',c.consequence),
            c.quote.length?keyValues(c.quote.map(q=>({label:q.label,content:q.value})),'zu-kv zu-quote'):null,
            node('h3','zu-subhead','Visible changes'),c.changes.length?keyValues(c.changes.map(x=>({label:x.label,content:node('span','zu-change',node('span','zu-before',x.before),node('span','zu-arrow',' → '),node('span','zu-after',x.after))})),'zu-kv zu-changes'):node('p','zu-help','No state changes are visible in this preview.'),
            node('h3','zu-subhead','Your request'),keyValues([...c.request,...c.who].map(r=>({label:r.label,content:r.value}))),
            node('p','zu-help',`Based on revision ${c.revision} of the state. Any edit cancels this confirmation.`),
            node('details','zu-disclosure',node('summary',null,`Visible request fields (${c.complete.length})`),keyValues(c.complete.map(r=>({label:r.label,content:r.value,muted:r.muted})))),
            record(c.record),node('div','zu-confirm-actions',confirm,back)].filter(Boolean));
          if(opened)heading.focus?.();});
        return section;
      }
      case 'status':{
        const section=node('section','zu-region zu-region--verdict'),live=node('div','zu-live'),delivery=node('p','zu-help'),pending=node('p','zu-help');let last;section.dataset.region='verdict';section.tabIndex=-1;
        live.setAttribute('role','status');live.setAttribute('aria-live','polite');live.setAttribute('aria-atomic','true');
        delivery.setAttribute('role','status');delivery.setAttribute('aria-live','polite');delivery.setAttribute('aria-atomic','true');section.append(live,delivery,pending);
        on(n=>{const s=n.nodes[path];if(same(s,last))return;last=s;live.replaceChildren(notice(s.tone,s.title,s.message));delivery.textContent=s.delivery;delivery.hidden=!s.delivery;pending.textContent=s.pending;pending.hidden=!s.pending;});
        return section;
      }
      case 'acting':{
        const box=node('div','zu-acting');box.setAttribute('role','group');box.setAttribute('aria-label',v.label);
        if(v.server){const value=node('p','zu-acting-value');box.append(node('p','zu-acting-label',v.label),value);on(n=>{value.textContent=n.nodes[path].text;});return box;}
        const name=node('input','zu-input');name.id='principal';name.type='text';name.maxLength=100;name.autocomplete='off';
        name.addEventListener('input',()=>handlers.principal(name.value));
        const fields=node('div','zu-acting-fields',labelled('principal','Your name',name));
        const controls=v.fields.map(f=>{const c=control(f);c.addEventListener('input',()=>handlers.context(f.index,c.value));fields.append(labelled(f.id,f.label,c,f.help));return c;});
        box.append(node('p','zu-acting-label',v.label),fields,node('p','zu-help','For this local page only. In the app, signing in sets these.'));
        on(n=>{const s=n.nodes[path];if(name.value!==s.principal)name.value=s.principal;name.disabled=n.phase==='running';
          s.fields.forEach((f,i)=>{if(controls[i].value!==f.value)controls[i].value=f.value;controls[i].disabled=n.phase==='running';});});
        return box;
      }
      case 'display':{
        const section=node('section','zu-region zu-display');section.dataset.display=ast.idiom;let last;
        on(n=>{const s=n.nodes[path];if(same(s,last))return;last=s;section.replaceChildren(...displayContent(ast.idiom,s));});
        return section;
      }
      case 'evidence':{
        const section=node('section','zu-evidence');section.dataset.region=ast.region;let last;
        on(n=>{const s=n.nodes[path];if(same(s,last))return;last=s;section.replaceChildren(...evidenceContent(ast.region,s));});
        return section;
      }
      default:throw Error('UiUnknownAstNode');
    }
  }
  // Each element names its tree node, so a browser check can compare it with the node's view.
  const tagged=build;
  build=(ast,path,context)=>{const el=tagged(ast,path,context);el.dataset.node=path;return el;};
  const element=build(ast,'$',{scope:null,idiom:null});
  return {element,update:view=>{for(const u of updaters)u(view);}};
}
