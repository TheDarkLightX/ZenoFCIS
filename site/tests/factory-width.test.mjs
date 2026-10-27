import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync,writeFileSync} from 'node:fs';
import {FACTORY_LAYOUT,GRID,factoryLayoutCSS,factoryMinimumWidths,checkLayout} from '../public/factory-layout.js';
const generated=new URL('../public/factory-layout.css',import.meta.url);
if(process.argv.includes('--write'))writeFileSync(generated,factoryLayoutCSS());
const css=readFileSync(new URL('../public/site.css',import.meta.url),'utf8');
test('factory_declared_minimum_width_fits_390px_from_shared_layout',()=>{
  assert.equal(readFileSync(generated,'utf8'),factoryLayoutCSS(),'generated CSS must match the shared datum');
  assert.match(readFileSync(new URL('../public/index.html',import.meta.url),'utf8'),/href="factory-layout.css"/);
  const widths=factoryMinimumWidths(390);
  assert.deepEqual(widths.map(r=>r.region),['controls','instrument','metrics','comparison','record','grid','ledger','witness']);
  assert.ok(widths.every(r=>r.available===350&&r.minimum<=350));
  assert.equal(widths.find(r=>r.region==='grid').minimum,26);
  // The canvas remains exact size and reachable via its own scroll container.
  assert.equal(GRID.width,612);assert.equal(checkLayout(),864);
  assert.match(css,/\.inventory-grid \{[^}]*max-width: 100%; overflow: auto;/);
  assert.match(factoryLayoutCSS(),/overflow-wrap: anywhere/);
  console.log(JSON.stringify({viewport:390,widths,canvas:GRID.width,grid:'scrollable; no cells scaled or removed'}));
});
test('width_check_refuses_intrinsic_ledger_and_oversized_declared_regions',()=>{
  // Before: the reserve value alone is an unbreakable max-content column,
  // alongside a label, with another 1.5rem left padding. Text cannot wrap it.
  const old=structuredClone(FACTORY_LAYOUT);
  old.regions.find(r=>r.name==='ledger').columns='minmax(0, 1fr) max-content';
  assert.throws(()=>factoryMinimumWidths(390,old),/ledger: unaccounted minimum width/);
  for(const region of FACTORY_LAYOUT.regions){
    const bad=structuredClone(FACTORY_LAYOUT);bad.regions.find(r=>r.name===region.name).insetRem=12;
    assert.throws(()=>factoryMinimumWidths(390,bad),new RegExp(`${region.name}: minimum .* exceeds 350px`));
  }
});
