import { mountInventoryGrid } from "./factory-grid.js";
import { human, inventoryInput, inventoryDecision, synthesisInput, yesNo, amount } from "./factory-format.js";

import { ACTS, initial, step, view, count, descriptor } from "./zenoui/presentation.js";

function element(tag, className, ...children) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.append(...children.filter((child) => child !== null && child !== undefined));
  return node;
}
// A row of headline figures: the number first, then what it counts.
function metrics(...items) {
  const list = element("dl","metrics");
  for (const [value,label,tone] of items) {
    list.append(element("div",tone ? `metric ${tone}` : "metric",element("dt",null,label),element("dd",null,value)));
  }
  return list;
}

// The three pairs of checks whose order the synthesis chooses (the sketch's holes).
const PAIRS = [
  { first:"model_ok", second:"price_fresh", names:["model","price"], reasons:["unapproved_model","stale_price"],
    fails:["the model is not approved","the price is stale"] },
  { first:"within_cap", second:"within_budget", names:["cap","budget"], reasons:["over_trade_cap","over_daily_budget"],
    fails:["the trade is over the cap","the daily budget is spent"] },
  { first:"slippage_ok", second:"reserve_ok", names:["slippage","reserve"], reasons:["slippage_too_wide","below_reserve"],
    fails:["slippage is too wide","the reserve would fall too low"] },
];
const orderOf = (choices) => PAIRS.map((pair,index) => `${pair.names[choices[index] ? 0 : 1]} first`);

function candidateList(output) {
  const list = element("ol","candidate-cards");
  const rows = [];
  for (let number = 0; number < 8; number++) {
    const choices = [number >> 2 & 1,number >> 1 & 1,number & 1];
    const state = element("span","candidate-state","waiting");
    const bar = element("div","candidate-bar",element("span"));
    bar.setAttribute("role","progressbar"); bar.setAttribute("aria-valuemin","0");
    bar.setAttribute("aria-valuemax","6144"); bar.setAttribute("aria-valuenow","0");
    bar.setAttribute("aria-label",`Order ${number + 1}: inputs checked`);
    const order = element("span","candidate-order",...orderOf(choices).map((text) => element("span",null,text)));
    const row = element("li",null,element("span","candidate-index",String(number + 1)),order,bar,state);
    row.dataset.candidate = number; list.append(row); rows.push({row,state,bar});
  }
  output.append(list);
  return (candidate) => {
    const view = rows[candidate.candidate];
    view.row.className = candidate.status;
    view.state.textContent = candidate.status === "selected"
      ? `all ${count(candidate.checked)} pass` : `refuted at ${count(candidate.checked)}`;
    view.bar.setAttribute("aria-valuenow",candidate.checked);
    view.bar.firstChild.style.width = `${100 * candidate.checked / 6144}%`;
  };
}
function synthesisExplanation(result) {
  const witness = result.first_counterexample, facts = result.inputs;
  const value = (name) => witness.input[facts.indexOf(`fact.${name}`)];
  const selected = result.candidates.find((candidate) => candidate.status === "selected");
  const refuted = result.candidates.find((candidate) => candidate.status === "refuted");
  const index = PAIRS.findIndex((pair) => value(pair.first) === 0 && value(pair.second) === 0);
  const answered = human(result.names.reason_ranks[witness.output[0]]);
  const card = element("div","counterexample",
    element("h4",null,`Why order ${refuted.candidate + 1} was refuted`),
    element("p","witness",synthesisInput(witness.input,result)));
  if (index >= 0) {
    const pair = PAIRS[index], allowed = pair.reasons[selected.choices[index] ? 0 : 1];
    const checkedFirst = pair.names[refuted.choices[index] ? 0 : 1];
    card.append(element("p",null,`Two checks fail at once here: ${pair.fails[0]}, and ${pair.fails[1]}. `
      + `This order checks the ${checkedFirst} first, so it refused the request as “${answered}”. `
      + `The contract does not allow that answer for this input; the selected order answers “${human(allowed)}”.`));
  } else {
    card.append(element("p",null,`This order refused the request as “${answered}”, which the contract does not allow for this input.`));
  }
  card.append(element("p","help","Only the facts that hold are listed; the rest are false."));
  return card;
}
function decisionComparison(output, witness, names) {
  const comparison = element("div","decision-comparison");
  const views = [witness.original,witness.mutant].map((decision) => inventoryDecision(decision,names));
  const differs = (key) => JSON.stringify(views[0][key]) !== JSON.stringify(views[1][key]);
  for (const [label,view,isMutant] of [["The rules as written",views[0],false],["With the bug",views[1],true]]) {
    const line = (key,text) => element("p",isMutant && differs(key) ? `${key} changed` : key,text);
    comparison.append(element("section",isMutant ? "mutant" : "original",element("h4",null,label),
      line("label",view.label),line("reason",view.reason),line("after",view.after),
      line("effects",view.effects.length ? `Queued: ${view.effects.join("; ")}.` : "Nothing queued."),
      element("p","law-summary",view.laws.length ? `Laws: ${view.laws.join("; ")}.` : "Stopped before the laws.")));
  }
  output.replaceChildren(comparison);
}
export function mountFactory(root) {
  let worker, pending, inventoryGrid, gridTask, selectedPanel, cellRecord;
  let state = initial('0', {motion:matchMedia('(prefers-reduced-motion: reduce)').matches?'reduce':'normal',theme:matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light',width:innerWidth});
  const controls = [...root.querySelectorAll('button[data-act]')];
  const regions = new Map();
  function resetRegions(act) {
    const output=root.querySelector(`[data-factory-act="${act}"] .factory-output`);
    const instrument=element('div','factory-instrument',element('p','help','No inputs checked yet.'));
    const metricsHost=element('div','factory-metrics');
    const comparison=element('div','factory-comparison',element('p','help','No comparison yet.'));
    const record=element('details','technical-record',element('summary',null,'Technical record'));
    const report=element('pre',null,'No result yet.');record.append(report);
    for(const [key,node] of [['instrument',instrument],['metrics',metricsHost],['comparison',comparison],['record',record]])node.dataset.region=key;
    output.replaceChildren(instrument,metricsHost,comparison,record);delete output.dataset.complete;
    const task={output,instrument,metricsHost,comparison,report,record,chunks:0,candidates:0};regions.set(act,task);return task;
  }
  function showMetrics(task,data) {
    const strip=metrics(...data.metrics.map(m=>[m.value,m.label,m.tone]));
    [...strip.children].forEach((node,i)=>{node.dataset.value=data.metrics[i].key;node.dataset.type=data.metrics[i].type;node.dataset.unit=data.metrics[i].unit;});
    task.metricsHost.replaceChildren(strip);
  }
  function gridFor(task,data) {
    const layout=element('div','inventory-layout'),host=element('div','inventory-map');
    selectedPanel=element('section','cell-inspection',element('h4',null,'Selected input'),element('p','help','Select a cell to see its decision.'));
    cellRecord=element('pre');task.record.append(cellRecord);layout.append(host,selectedPanel);task.instrument.replaceChildren(layout);
    const identity=state.identity;
    inventoryGrid=mountInventoryGrid(host,data.result?.mutant??data.draft.mutant,state.run,(cell)=>dispatch({type:'Select',identity,ordinal:cell.ordinal}));
    gridTask=task;return inventoryGrid;
  }
  function renderSelection(data) {
    if(!selectedPanel||!data.selection)return;
    const selection=data.selection,r=data.tasks.inventory.result,names=r.names,cell=r.grid.cells[selection.ordinal];
    const heading=cell.different?'A decision the bug changed':'Selected input';
    const text=element('p','selected-input',inventoryInput(cell.input,names));
    selectedPanel.replaceChildren(element('h4',null,heading),text);
    delete selectedPanel.dataset.ready;
    if(selection.error)selectedPanel.append(element('p',null,selection.error));
    else if(!selection.result)selectedPanel.append(element('p','help','Deciding this input…'));
    else {
      const decisions=element('div');
      if(r.mutant==='none') {
        const detail=inventoryDecision(selection.result.original,names);
        decisions.append(element('div','decision-single',element('p','label',detail.label),element('p','reason',detail.reason),element('p','after',detail.after),
          element('p','effects',detail.effects.length?`Queued: ${detail.effects.join('; ')}.`:'Nothing queued.'),element('p','law-summary',detail.laws.length?`Laws: ${detail.laws.join('; ')}.`:'Stopped before the laws.')));
      } else decisionComparison(decisions,selection.result,names);
      selectedPanel.append(decisions);selectedPanel.dataset.ready='true';selectedPanel.dataset.ordinal=selection.ordinal;selectedPanel.dataset.mutant=r.mutant;
      cellRecord.textContent=JSON.stringify({selected_cell:selection.ordinal,...selection.result},null,2);
      root.dispatchEvent(new CustomEvent('factory-detail',{detail:{mutant:r.mutant,ordinal:selection.ordinal,result:selection.result}}));
    }
  }
  function paint(data,event) {
    for(const button of controls)button.disabled=!data.actions.find(a=>a.act===button.dataset.act).enabled;
    root.querySelector('.mutant-choice').disabled=data.busy;
    inventoryGrid?.setEnabled(data.gridEnabled);
    for(const act of ACTS) {
      const taskData=data.tasks[act];let task=regions.get(act);
      if(!task||event.type==='Start'&&data.act===act)task=resetRegions(act);
      root.querySelector(`[data-factory-act="${act}"] .factory-status`).textContent=taskData.status;
      showMetrics(task,taskData);
      if(taskData.phase==='running') {
        if(act==='synthesis') {
          if(!task.updateCandidate){task.instrument.replaceChildren();task.updateCandidate=candidateList(task.instrument);}
          for(const c of taskData.candidates.slice(task.candidates))task.updateCandidate(c);
          task.candidates=taskData.candidates.length;
        }
        if(act==='inventory'&&taskData.chunks.length>task.chunks) {
          const grid=gridTask===task?inventoryGrid:gridFor(task,taskData);
          for(const chunk of taskData.chunks.slice(task.chunks))grid.append(chunk);
          task.chunks=taskData.chunks.length;
        }
      }
      if(event.type==='Result'&&data.act===act&&taskData.phase==='done') {
        task.comparison.replaceChildren(element('p','verdict',taskData.summary));
        task.report.textContent=JSON.stringify(taskData.record,null,2);
        if(act==='inventory') {
          const grid=gridTask===task?inventoryGrid:gridFor(task,taskData);
          task.output.querySelector('.inventory-grid').dataset.progressChunks=taskData.chunks.length;
          grid.finish(taskData.result);
        } else {
          if(act==='synthesis')for(const c of taskData.result.candidates)task.updateCandidate(c);
          render(task.comparison,taskData);
        }
        task.output.dataset.complete='true';
      }
    }
    if(['Select','CellResult','Fail'].includes(event.type))renderSelection(data);
  }
  function connect() {
    const connectionEpoch=state.epoch;
    worker=new Worker(new URL('./factory-worker.js',import.meta.url),{type:'module'});
    worker.onmessage=({data})=>{
      try {
        const event=data.error?{type:'Fail',identity:data.identity,ordinal:data.ordinal,message:data.error}:data.detail?{type:'CellResult',identity:data.identity,ordinal:data.ordinal,result:data.detail}:data.result?{type:'Result',identity:data.identity,result:data.result,elapsed:data.elapsed}:{type:'Progress',identity:data.identity,progress:data.progress,candidate:data.candidate};
        dispatch(event);
      }catch(error){dispatch({type:'Fail',identity:data.identity,message:error.message});}
    };
    worker.onerror=()=>dispatch({type:'Fail',identity:{...state.identity,epoch:connectionEpoch},message:'The worker could not run. Reload and try again.'});
  }
  function dispatch(event) {
    const before=state,result=step(state,event);
    if(result.state===before)return false;
    state=result.state;
    paint(result.view,event);
    if(event.type==='Result'&&pending) {
      clearTimeout(pending.timer);const task=pending;pending=null;
      const detail={identity:state.identity,result:result.view.tasks[state.act].result,elapsed:result.view.tasks[state.act].elapsed};
      root.dispatchEvent(new CustomEvent('factory-result',{detail}));task.resolve(detail);
    }
    if((event.type==='Fail'&&event.ordinal===undefined)||event.type==='Cancel') {
      if(pending){clearTimeout(pending.timer);pending.reject(new Error(event.message??'Cancelled'));pending=null;}
      worker?.terminate();worker=null;
    }
    for(const intent of result.intents) {
      if(intent.kind==='cancel')continue;
      if(!worker)connect();
      worker.postMessage(intent);
    }
    return true;
  }
  function run(act) {
    const kind=act.split('/')[0],section=root.querySelector(`[data-factory-act="${kind}"]`);
    const draft=kind==='money'?Object.fromEntries([...section.querySelectorAll('input[data-money]')].map(input=>[input.dataset.money,input.value])):kind==='inventory'?{mutant:act.split('/')[1]}:{};
    if(state.phase==='running')return Promise.reject(new Error('A check is already running'));
    return new Promise((resolve,reject)=>{
      pending={resolve,reject};
      if(kind==='inventory'){inventoryGrid?.dispose();inventoryGrid=null;gridTask=null;selectedPanel=null;}
      if(!dispatch({type:'Start',act,draft})){pending=null;reject(new Error('Check the input bounds.'));return;}
      const identity=state.identity;
      pending.timer=setTimeout(()=>dispatch({type:'Fail',identity,message:'The check timed out; no result is claimed'}),10000);
    });
  }
  for(const button of controls)button.addEventListener('click',()=>run(button.dataset.act==='inventory'?`inventory/${root.querySelector('.mutant-choice').value}`:button.dataset.act).catch(()=>{}));
  for(const link of root.querySelectorAll('[data-open-example]'))link.addEventListener('click',()=>{const example=document.getElementById(link.dataset.openExample);if(example)example.open=true;});
  for(const input of root.querySelectorAll('input[data-money]'))input.addEventListener('input',()=>dispatch({type:'DraftChanged',act:'money',draft:Object.fromEntries([...root.querySelectorAll('input[data-money]')].map(input=>[input.dataset.money,input.value]))}));
  root.querySelector('.mutant-choice').addEventListener('change',()=>dispatch({type:'DraftChanged',act:'inventory',draft:{mutant:root.querySelector('.mutant-choice').value}}));
  const listeners=[];
  for(const [query,key,values] of [['(prefers-reduced-motion: reduce)','motion',['normal','reduce']],['(prefers-color-scheme: dark)','theme',['light','dark']]]) {
    const media=matchMedia(query),listener=()=>dispatch({type:'PreferencesChanged',preferences:{[key]:values[Number(media.matches)]}});
    media.addEventListener('change',listener);listeners.push(()=>media.removeEventListener('change',listener));
  }
  paint(view(state),{type:'Initial'});
  return {run,dispose:()=>{dispatch({type:'Cancel'});inventoryGrid?.dispose();listeners.forEach(remove=>remove());}};
}

function nodeStrip(label, nodes, tone) {
  const strip = element("div",`node-strip ${tone}`,element("span","node-label",label));
  const cells = element("span","node-cells");
  for (let index = 0; index < nodes; index++) cells.append(element("span","node"));
  cells.setAttribute("aria-hidden","true");
  strip.append(cells);
  return strip;
}
function render(output,data) {
  const result=data.result;
  if (result.act === "synthesis") {
    output.append(synthesisExplanation(result));
  }
  if (result.act === "search") {
    const w = result.first_difference;
    output.append(element("div","node-strips",nodeStrip("Original",result.before_nodes,"before"),nodeStrip("Found",result.after_nodes,"after")),
      element("div","counterexample",element("h4",null,"A wrong proposal is refused"),
        element("p",null,`A proposal with two outputs swapped is checked first. The checker refuses it at input ${w.ordinal + 1} of ${result.domain}:`),
        element("dl","witness-table",
          element("dt",null,"Inputs"),element("dd",null,w.input.map(yesNo).join(" · ")),
          element("dt",null,"Original answers"),element("dd",null,w.original.map(yesNo).join(" · ")),
          element("dt",null,"Proposal answers"),element("dd","changed",w.swapped.map(yesNo).join(" · "))),
        element("p","help","This kernel declares no names for its inputs and outputs, so they are shown in order.")));
  }
  if (result.act === "money") {
    const [fee,net,out,reserveIn,reserveOut,k] = result.outputs;
    const [amountIn,feeBps] = result.input;
    output.append(element("dl","ledger-table",
      element("dt",null,`${descriptor("fee").label}, ${count(feeBps)} basis points of ${count(amountIn)} input atoms`),element("dd",null,`${count(fee)} input atoms`),
      element("dt",null,descriptor("net").label),element("dd",null,`${count(net)} input atoms`),
      element("dt","strong",descriptor("out").label),element("dd","strong",`${count(out)} output atoms`),
      element("dt",null,"Reserves after"),element("dd",null,`${count(reserveIn)} input atoms · ${count(reserveOut)} output atoms`),
      element("dt",null,"Product of reserves"),element("dd",k ? "good" : "warn",k ? "did not decrease" : "decreased: refused")),
      element("div","counterexample",element("h4",null,"A result that cannot fit is refused"),
        element("p",null,`The same calculation with an input reserve of ${amount(result.overflow.input[2])} and a net amount of ${result.overflow.input[0]}: their sum does not fit.`),
        element("p",null,`The library refuses after ${result.overflow.steps} instructions and returns no output, rather than a wrapped number.`)));
  }

}
const root = document.querySelector("#factory");
if (root) mountFactory(root);
