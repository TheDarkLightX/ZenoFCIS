import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { GRID,cellPosition,cellOrdinal,paintInventory } from "../public/factory-grid.js";
import { inventoryInput,inventoryDecision,synthesisInput,synthesisOutput,booleanTuple,amount } from "../public/factory-format.js";
import { checkGrid } from "./harness/grid-check.js";
const native=JSON.parse(readFileSync(new URL("harness/factory-native.json",import.meta.url)));
const css=readFileSync(new URL("../public/site.css",import.meta.url),"utf8");
function palette(dark) {
  const text=dark?css.slice(css.indexOf("@media (prefers-color-scheme: dark)")):css;
  const color=(name)=>text.match(new RegExp(`--${name}: (#[a-f0-9]+)`))[1];
  return {paper:color("paper"),ink:color("ink"),muted:color("muted"),line:color("line"),accept:color("accept-ink"),reject:color("reject-ink"),failure:color("failure-ink"),difference:color("failure-ink")};
}
const rgb=(hex)=>[1,3,5].map(i=>parseInt(hex.slice(i,i+2),16));
function snapshot(result,painted,dark=false) {
  const colors=palette(dark),rectangles=[];
  const context={fillRect(x,y,w,h){rectangles.push({x,y,w,h,color:this.fillStyle});},fillText(){},strokeRect(){},beginPath(){},arc(){},stroke(){},setLineDash(){}};
  paintInventory(context,painted.grid.cells,painted.names,colors,painted.grid.first_ordinal,null);
  const pixels=result.grid.cells.map((cell)=>{const {x,y}=cellPosition(cell.input);return rgb(rectangles.findLast(r=>x+4>=r.x&&x+4<r.x+r.w&&y+4>=r.y&&y+4<r.y+r.h).color);});
  return {count:864,mutant:result.mutant,differences:result.differences,first:result.grid.first_ordinal,cellSize:8,colors:Object.fromEntries(Object.entries(colors).map(([key,v])=>[key,rgb(v)])),pixels,marker:rgb(colors.ink)};
}
test("grid_draw_commands_match_all_native_cells_in_both_themes",()=>{
  for(const result of Object.values(native).filter(r=>r.act==="inventory")) {
    for(const dark of [false,true]) assert.equal(checkGrid(snapshot(result,result,dark),result).cells,864);
    const positions=new Set();
    for(const cell of result.grid.cells) {
      const p=cellPosition(cell.input); assert.equal(cellOrdinal(p.row,p.column),cell.ordinal);
      assert.ok(p.x>=0 && p.x+8<=GRID.width && p.y>=0 && p.y+8<=GRID.height);
      positions.add(`${p.x},${p.y}`);
    }
    assert.equal(positions.size,864);
  }
});
test("stale_grid_colours_fail_native_grid_check",()=>{
  const current=native["inventory/m0061"],stale=native["inventory/none"];
  assert.throws(()=>checkGrid(snapshot(current,stale),current),/grid_matches_native_result: cell .*another result/);
});
test("declared_names_and_typed_values_replace_numeric_vectors",()=>{
  const result=native["inventory/m0061"],w=result.first_difference;
  assert.equal(inventoryInput(w.input,result.names),"available 3 · reserved 2 · action Ship · quantity 2 · operator authorized yes");
  assert.equal(inventoryDecision(w.original,result.names).after,"After: available 3 · reserved 0.");
  assert.deepEqual(inventoryDecision(w.original,result.names).effects,["shipment to warehouse: shipped_units 2"]);
  const synthesis=native.synthesis;
  assert.equal(synthesisInput(synthesis.first_counterexample.input,synthesis),"action ProposeSwap · caller ok yes · clock ok yes");
  assert.equal(synthesisOutput([4],synthesis.names),"Rejected: stale price");
  assert.equal(booleanTuple([0,1],"input"),"input 1: no · input 2: yes");
  assert.match(amount(native.money.overflow.input[2]),/^2\^128 − 1/);
  assert.deepEqual(synthesis.candidates.map(c=>c.checked),[1537,1537,1537,1537,1921,1921,1933,6144]);
});
