import { mountInventoryGrid } from "./factory-grid.js";
import { human, inventoryInput, inventoryDecision, synthesisInput, yesNo, amount } from "./factory-format.js";

const count = (value) => Number(value).toLocaleString("en-US");

function element(tag, className, ...children) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.append(...children.filter((child) => child !== null && child !== undefined));
  return node;
}
function technicalRecord(output, result) {
  const details = element("details","technical-record",element("summary",null,"Technical record"));
  details.append(element("pre",null,JSON.stringify(result,null,2)));
  output.append(details);
  return details;
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
  let worker, sequence = 0, pending, inspection, inventoryGrid, inventoryRun;
  const controls = [...root.querySelectorAll("button[data-act]")];
  const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
  function setBusy(busy) {
    for (const button of controls) button.disabled = busy;
    root.querySelector(".mutant-choice").disabled = busy;
    inventoryGrid?.setEnabled(!busy);
  }
  function cancelInspection() {
    if (inspection) { inspection.reject(new Error("Cell selection changed")); inspection = null; }
  }
  function inspect(mutant, ordinal, runId) {
    cancelInspection();
    return new Promise((resolve,reject) => {
      const id = ++sequence;
      inspection = { id,runId,resolve,reject };
      worker.postMessage({ id,kind:"inspect",mutant,ordinal });
    });
  }
  function gridFor(task, mutant) {
    const layout = element("div","inventory-layout");
    const gridHost = element("div","inventory-map");
    const selected = element("section","cell-inspection",element("h4",null,"Selected input"),element("p","help","Select a cell to see its decision."));
    const raw = element("details","technical-record",element("summary",null,"Technical record"));
    const report = element("pre"), cellRecord = element("pre"); raw.append(report,cellRecord);
    layout.append(gridHost,selected);
    task.output.append(layout,raw);
    task.record = report;
    let selection = 0;
    const grid = mountInventoryGrid(gridHost,mutant,task.id,async (cell,names) => {
      const version = ++selection;
      delete selected.dataset.ready;
      const heading = cell.different ? "A decision the bug changed" : "Selected input";
      selected.replaceChildren(element("h4",null,heading),element("p","selected-input",inventoryInput(cell.input,names)),element("p","help","Deciding this input…"));
      try {
        const detail = await inspect(mutant,cell.ordinal,task.id);
        if (version !== selection || inventoryRun !== task.id) return;
        if (JSON.stringify(detail.input) !== JSON.stringify(cell.input)) throw new Error("Cell report belongs to another input");
        const decisions = element("div");
        if (mutant === "none") {
          const view = inventoryDecision(detail.original,names);
          decisions.append(element("div","decision-single",element("p","label",view.label),element("p","reason",view.reason),
            element("p","after",view.after),element("p","effects",view.effects.length ? `Queued: ${view.effects.join("; ")}.` : "Nothing queued."),
            element("p","law-summary",view.laws.length ? `Laws: ${view.laws.join("; ")}.` : "Stopped before the laws.")));
        } else decisionComparison(decisions,detail,names);
        selected.replaceChildren(element("h4",null,heading),element("p","selected-input",inventoryInput(cell.input,names)),decisions);
        selected.dataset.ready = "true"; selected.dataset.ordinal = cell.ordinal; selected.dataset.mutant = mutant;
        cellRecord.textContent = JSON.stringify({selected_cell:cell.ordinal,...detail},null,2);
        root.dispatchEvent(new CustomEvent("factory-detail",{detail:{mutant,ordinal:cell.ordinal,result:detail}}));
      } catch (error) { if (version === selection && inventoryRun === task.id) selected.replaceChildren(element("p",null,error.message)); }
    });
    task.grid = grid; inventoryGrid = grid;
    return grid;
  }
  function connect() {
    worker = new Worker(new URL("./factory-worker.js",import.meta.url),{type:"module"});
    worker.onmessage = ({data}) => {
      if (inspection?.id === data.id) {
        const current = inspection; inspection = null;
        if (current.runId !== inventoryRun) current.reject(new Error("Cell belongs to an earlier run"));
        else if (data.error) current.reject(new Error(data.error));
        else current.resolve(data.detail);
        return;
      }
      if (!pending || data.id !== pending.id) return;
      const task = pending;
      try {
        if (data.candidate) { pending.candidateCount++; if (!reducedMotion.matches) pending.updateCandidate(data.candidate); return; }
        if (data.progress) {
          pending.chunks++;
          if (!reducedMotion.matches) (pending.grid ?? gridFor(pending,data.progress.mutant)).append(data.progress);
          return;
        }
        if (data.error) throw new Error(data.error);
        clearTimeout(task.timer); pending = null; setBusy(false);
        if (data.result.act === "inventory") {
          const result = data.result, mutant = result.mutant;
          const changed = mutant === "none" ? null : result.differences;
          const summary = mutant === "none"
            ? `All ${count(result.domain)} decisions satisfy every law that applies to them.`
            : changed === 0 ? "This change alters no decision on any of the 864 inputs: the two versions are equivalent."
            : `The bug changes ${count(changed)} of ${count(result.domain)} decisions. The first is ringed in the grid, and its two decisions are compared in the panel.`;
          task.output.prepend(metrics(
            [count(result.domain),"inputs decided"],
            [count(mutant === "none" ? result.passed : result.mutant_passed),"within every applicable law", "good"],
            ...(changed === null ? [[count(result.law_checks),"law checks passed"]] : [[count(changed),"decisions changed",changed ? "warn" : "good"]]),
            [`${data.elapsed.toFixed(0)} ms`,"in this browser"]),
            element("p","verdict",summary));
          const grid = task.grid ?? gridFor(task,mutant);
          task.output.querySelector(".inventory-grid").dataset.progressChunks = task.chunks;
          task.record.textContent = JSON.stringify(result,null,2);
          grid.finish(result);
        } else {
          if (data.result.act === "synthesis") for (const candidate of data.result.candidates) task.updateCandidate(candidate);
          render(task.output,data.result,data.elapsed);
        }
        task.status.textContent = `Done in ${data.elapsed.toFixed(0)} ms, in this browser.`;
        task.output.dataset.complete = "true";
        root.dispatchEvent(new CustomEvent("factory-result",{detail:data})); task.resolve(data);
      } catch (error) {
        if (pending) fail(error.message);
        else { task.status.textContent = `Stopped: ${error.message}`; task.reject(error); }
      }
    };
    worker.onerror = () => fail("The worker could not run. Reload and try again.");
  }
  function fail(message) {
    cancelInspection();
    if (!pending) return;
    const task = pending; pending = null; clearTimeout(task.timer);
    worker.terminate(); worker = null; setBusy(false);
    task.status.textContent = `Stopped: ${message}`; task.reject(new Error(message));
  }
  function run(act) {
    if (pending) return Promise.reject(new Error("A check is already running"));
    cancelInspection();
    const section = root.querySelector(`[data-factory-act="${act.split("/")[0]}"]`);
    const status = section.querySelector(".factory-status"), output = section.querySelector(".factory-output");
    if (act.startsWith("inventory/")) { inventoryGrid?.dispose(); inventoryGrid = null; }
    output.replaceChildren(); delete output.dataset.complete;
    status.textContent = "Running the library in a background worker…"; setBusy(true);
    if (!worker) connect();
    return new Promise((resolve,reject) => {
      const id = ++sequence, timer = setTimeout(() => fail("the check timed out; no result is claimed."),10000);
      if (act.startsWith("inventory/")) inventoryRun = id;
      pending = {id,status,output,resolve,reject,timer,chunks:0,candidateCount:0};
      if (act === "synthesis") pending.updateCandidate = candidateList(output);
      const options = act === "money" ? Object.fromEntries([...section.querySelectorAll("input[data-money]")].map((input) => [input.dataset.money,input.value])) : {};
      worker.postMessage({id,act,options});
    });
  }
  for (const button of controls) button.addEventListener("click",() => run(button.dataset.act === "inventory" ? `inventory/${root.querySelector(".mutant-choice").value}` : button.dataset.act).catch(() => {}));
  for (const link of root.querySelectorAll("[data-open-example]")) link.addEventListener("click",() => {
    const example = document.getElementById(link.dataset.openExample);
    if (example && !example.open) example.open = true;
  });
  return {run,dispose:() => {fail("cancelled.");inventoryGrid?.dispose();worker?.terminate();}};
}
function nodeStrip(label, nodes, tone) {
  const strip = element("div",`node-strip ${tone}`,element("span","node-label",label));
  const cells = element("span","node-cells");
  for (let index = 0; index < nodes; index++) cells.append(element("span","node"));
  cells.setAttribute("aria-hidden","true");
  strip.append(cells);
  return strip;
}
function render(output,result,elapsed) {
  if (result.act === "synthesis") {
    const refuted = result.candidates.filter((candidate) => candidate.status === "refuted").length;
    const selected = result.candidates.find((candidate) => candidate.status === "selected");
    output.append(metrics(
      [String(result.assignments),"orders tried"],
      [String(refuted),"refuted by an exact input","warn"],
      [`${count(result.passed)} / ${count(result.domain)}`,"inputs pass for the chosen order","good"],
      [`${elapsed.toFixed(0)} ms`,"in this browser"]),
      element("p","verdict",`Order ${selected.candidate + 1} is chosen: ${orderOf(selected.choices).join(", ")}. It is right on every one of the ${count(result.domain)} inputs.`),
      synthesisExplanation(result));
  }
  if (result.act === "search") {
    const w = result.first_difference;
    output.append(metrics(
      [`${result.before_nodes} → ${result.after_nodes}`,"nodes"],
      [`${result.passed} / ${result.domain}`,"inputs give the same answers","good"],
      [count(result.work),"search work units"],
      [`${elapsed.toFixed(0)} ms`,"in this browser"]),
      element("div","node-strips",nodeStrip("Original",result.before_nodes,"before"),nodeStrip("Found",result.after_nodes,"after")),
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
      element("dt",null,`Fee, ${count(feeBps)} basis points of ${count(amountIn)}, rounded up`),element("dd",null,count(fee)),
      element("dt",null,"Net amount in"),element("dd",null,count(net)),
      element("dt","strong","Amount out, rounded down"),element("dd","strong",count(out)),
      element("dt",null,"Reserves after"),element("dd",null,`${count(reserveIn)} · ${count(reserveOut)}`),
      element("dt",null,"Product of reserves"),element("dd",k ? "good" : "warn",k ? "did not decrease" : "decreased: refused")),
      element("div","counterexample",element("h4",null,"A result that cannot fit is refused"),
        element("p",null,`The same calculation with an input reserve of ${amount(result.overflow.input[2])} and a net amount of ${result.overflow.input[0]}: their sum does not fit.`),
        element("p",null,`The library refuses after ${result.overflow.steps} instructions and returns no output, rather than a wrapped number.`)));
  }
  technicalRecord(output,result);
}
const root = document.querySelector("#factory");
if (root) mountFactory(root);
