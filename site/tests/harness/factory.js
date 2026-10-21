import { captureGrid,checkGrid } from "./grid-check.js";
const parameters = new URLSearchParams(location.search);
const output = document.getElementById("results");
const frame = document.querySelector("iframe");
const errors = [],results = {};
const canonical = (v) => JSON.stringify(v,(_,value)=>value&&typeof value==="object"&&!Array.isArray(value)?Object.fromEntries(Object.keys(value).sort().map(k=>[k,value[k]])):value);
const delay=(ms)=>new Promise(resolve=>setTimeout(resolve,ms));
async function waitFor(test,label) {
  const deadline=performance.now()+10000;
  while(!test()) {if(performance.now()>deadline)throw new Error(label);await delay(20);}
}
function require(condition,message) {if(!condition)throw new Error(message);}
try {
  const base = new URL(parameters.get("base") ?? "/ZenoFCIS/", location.href);
  const layout = await import(new URL("factory-layout.js", base));
  const point = layout.cellPosition;
  const expected = await (await fetch("factory-native.json")).json();
  const detailOracle = await (await fetch("inventory-details-native.json")).json();
  const loaded = new Promise(resolve=>frame.addEventListener("load",resolve,{once:true}));
  frame.src=parameters.get("base")??"/ZenoFCIS/";await loaded;
  const doc=frame.contentDocument,root=doc.getElementById("factory");
  await waitFor(()=>root.querySelectorAll('[data-region]').length===16,'L1 incomplete initial presentation');
  for(const section of root.querySelectorAll('[data-factory-act]')) {
    require(section.querySelectorAll('[data-region]').length===4,'L1 missing initial region');
    require(section.querySelector('[data-value="checked"] dd')?.textContent==='0','L1 initial checked count is not zero');
  }
  let baselineBitmap;
  const rank=(act)=>act==="synthesis"?0:act==="inventory/none"?1:act.startsWith("inventory/")?2:act==="search"?3:4;
  for(const [act,oracle] of Object.entries(expected).sort(([left],[right])=>rank(left)-rank(right))) {
    let ticks=0,last=performance.now(),gap=0;
    let partialGrid=false;
    const observer=new frame.contentWindow.MutationObserver(()=>{
      const count=Number(root.querySelector(".inventory-grid")?.dataset.cells);
      if(count>0&&count<864)partialGrid=true;
    });
    observer.observe(root,{subtree:true,childList:true,attributes:true,attributeFilter:["data-cells"]});
    const timer=setInterval(()=>{ticks++;const now=performance.now();gap=Math.max(gap,now-last);last=now;},10);
    const complete=new Promise((resolve,reject)=>{
      const timeout=setTimeout(()=>reject(new Error(`${act}: worker did not complete`)),10000);
      root.addEventListener("factory-result",({detail})=>{clearTimeout(timeout);resolve(detail);},{once:true});
    });
    const [name,mutant]=act.split("/");
    if(mutant)root.querySelector(".mutant-choice").value=mutant;
    root.querySelector(`[data-act="${name}"]`).focus();
    document.body.dataset.keyboard=act;
    const data=await complete;clearInterval(timer);observer.disconnect();
    require(canonical(data.result)===canonical(oracle),`${act}: native parity mismatch`);
    require(data.elapsed<3000,`${act}: exceeded 3 seconds: ${data.elapsed}`);
    require(!((ticks===0&&data.elapsed>50)||gap>250),`${act}: UI responsiveness failed: ${ticks} ticks, ${gap} ms gap`);
    const section=root.querySelector(`[data-factory-act="${name}"]`);
    require(section.querySelector(".factory-output").dataset.complete==="true",`${act}: result not rendered`);
    require(section.querySelector(".technical-record:not([open])"),`${act}: raw technical record is not collapsed`);
    for(const metric of section.querySelectorAll('.metrics .metric')) {
      require(metric.dataset.value&&metric.dataset.type&&metric.dataset.unit,'L5 displayed metric lacks name, type or unit');
    }
    const entry={elapsed:data.elapsed,ticks,maxGap:gap,parity:true};
    if(name==="synthesis") {
      const cards=[...section.querySelectorAll(".candidate-cards > li")];
      require(cards.length===8,"synthesis needs eight candidate cards");
      for(const [index,card] of cards.entries()) require(Number(card.querySelector('[role="progressbar"]').getAttribute("aria-valuenow"))===oracle.candidates[index].checked,"candidate bar disagrees with native work");
      await waitFor(()=>cards.every((card,index)=>{
        const bar=card.querySelector('[role="progressbar"]');
        return Math.abs(bar.firstElementChild.getBoundingClientRect().width/bar.getBoundingClientRect().width-oracle.candidates[index].checked/6144)<0.01;
      }),"candidate bar width disagrees with native work");
      require(section.querySelector(".counterexample").textContent.includes("action ProposeSwap"),"synthesis witness is not named");
    }
    if(name==="inventory") {
      const grid=section.querySelector(".inventory-grid"),canvas=grid.querySelector("canvas"),context=canvas.getContext("2d");
      require(Number(grid.dataset.progressChunks)===18,"grid did not receive every bounded progress chunk");
      require(partialGrid,"L3 both motion settings must render the same progress evidence");
      if(parameters.get("control")==="stale-grid"&&mutant==="m0002")context.putImageData(baselineBitmap,0,0);
      entry.grid=checkGrid(captureGrid(doc,oracle,layout),oracle);
      if(mutant==="none")baselineBitmap=context.getImageData(0,0,canvas.width,canvas.height);
      if(mutant==="m0049") {
        const current=context.getImageData(0,0,canvas.width,canvas.height).data;
        require(current.every((value,index)=>value===baselineBitmap.data[index]),"equivalent mutant changed the grid");
      }
      const selected=section.querySelector(".cell-inspection");
      async function checkSelection(ordinal) {
        await waitFor(()=>selected.dataset.ready==="true"&&Number(selected.dataset.ordinal)===ordinal,"selected cell was not decided");
        const raw=JSON.parse(section.querySelector(".technical-record > pre:last-child").textContent);
        const {selected_cell,...decision}=raw;
        require(selected_cell===ordinal&&canonical(decision)===canonical(detailOracle[`${mutant}/${ordinal}`]),"cell inspection differs from native authority");
      }
      await checkSelection(oracle.grid.first_ordinal??0);
      const initial=oracle.grid.cells[oracle.grid.first_ordinal??0].input;
      const column=Math.min(47,(initial[2]-150)*12+initial[4]*6+initial[0]+1);
      const target=oracle.grid.cells.find(cell=>cell.input[0]===column%6&&cell.input[1]===initial[1]&&cell.input[2]===150+Math.floor(column/12)&&cell.input[3]===initial[3]&&cell.input[4]===Math.floor(column%12/6));
      grid.focus({preventScroll:true});document.body.dataset.keyboardKey="ArrowRight";document.body.dataset.keyboard=`grid-${act}`;
      await checkSelection(target.ordinal);
      // Exercise the pointer coordinate path as well as native keyboard input.
      const mouseCell=oracle.grid.cells[495],p=point(mouseCell.input),box=canvas.getBoundingClientRect();
      canvas.dispatchEvent(new frame.contentWindow.MouseEvent("click",{bubbles:true,clientX:box.left+p.x+4,clientY:box.top+p.y+4}));
      await checkSelection(495);
      require(selected.textContent.includes("action Ship"),"selected action still uses its numeric variant");
      entry.keyboard=true;entry.pointer=true;entry.partialGrid=partialGrid;
    }
    results[act]=entry;
  }
  if(doc.documentElement.scrollWidth>frame.clientWidth){
    const boxes=[...root.querySelectorAll('[data-region], .act-controls, .ledger-table, .mutation-label')].map(node=>{
      const box=node.getBoundingClientRect();return {region:node.dataset.region??node.className,left:box.left,right:box.right,width:box.width,scroll:node.scrollWidth};
    });
    throw new Error(`page overflows phone width: ${JSON.stringify({viewport:frame.clientWidth,page:doc.documentElement.scrollWidth,boxes})}`);
  }
  const challenge=doc.querySelector('details[data-template="agent-treasury-guard"]');challenge.open=true;
  await waitFor(()=>challenge.querySelector(".challenge-score"),"treasury scoreboard did not load");
  require(!challenge.querySelector(".challenge-about").open,"long treasury description is not collapsed");
  require(challenge.querySelector(".demo").firstElementChild.classList.contains("challenge-score"),"scoreboard does not lead the challenge");
  require(challenge.querySelectorAll(".challenge-counters strong").length===3,"challenge needs three live counters");
} catch(error) {errors.push(error.message);}
output.textContent=JSON.stringify({results,errors});
