'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {documentFrom}=require('./library_dom.cjs');
const html=fs.readFileSync(path.join(__dirname,'../lens_assets/index.html'),'utf8'),script=fs.readFileSync(path.join(__dirname,'../lens_assets/app.js'),'utf8');
const packet=JSON.parse(fs.readFileSync(process.env.ZAL_CATEGORY_PACKET,'utf8'));
function mount(){const document=documentFrom(html),window={ZAL_PACKET:packet,location:{hash:''},matchMedia:()=>({matches:false}),addEventListener(){},history:{replaceState(){},pushState(){}}};vm.runInNewContext(script,{window,document,Event,URLSearchParams,console,module:{exports:{}}});return document;}
function select(doc,operation,status='CheckedAt'){const subject=packet.subjects.find(s=>s.lenses.some(v=>v.category?.application.operation===operation&&v.category.record.status===status)),lens=subject.lenses.find(v=>v.category?.application.operation===operation&&v.category.record.status===status);const picker=doc.getElementById('entry-select');picker.value=subject.object.key;picker.dispatchEvent(new Event('change'));doc.querySelector('[data-lens="'+lens.id+'"]').click();return{subject,lens};}
test('actual category controls expose typed queries and keep source meaning adjacent',()=>{
  const doc=mount();assert.equal(doc.getElementById('app').hidden,false);const {subject}=select(doc,'compose-path');
  assert.match(doc.getElementById('status-explanation').textContent,/Recorded result: CheckedAt/);
  assert.match(doc.getElementById('status-explanation').textContent,/fresh explicit-table oracle/);
  assert.match(doc.getElementById('status-explanation').textContent,/Source interpretation: ExactRepresentation/);
  assert.match(doc.getElementById('meaning-bound').textContent,/do not prove/);
  doc.getElementById('category-before').value='01';doc.getElementById('category-after').value='12';doc.getElementById('category-compose').click();
  assert.match(doc.getElementById('category-operation-result').textContent,/12 after 01 = 02/);
  doc.getElementById('category-before').value='12';doc.getElementById('category-after').value='01';doc.getElementById('category-compose').click();
  assert.match(doc.getElementById('category-operation-result').textContent,/Query refused: Incompatible typed/);
  doc.getElementById('restore-button').click();assert.equal(doc.getElementById('source-text').textContent,subject.object._sourceText);assert.equal(doc.getElementById('view-title').textContent,'Retained authored source');
});
test('functor fibers and symbolic hop controls are real recorded data',()=>{
  const doc=mount();select(doc,'compose-functors');assert.match(doc.getElementById('category-operation-result').textContent,/preimage/);
  select(doc,'compose-symbolic','Authored');assert.equal(doc.getElementById('view-status').textContent,'Authored symbolic composition');
  const hop=doc.getElementById('category-hop');hop.value='1';hop.dispatchEvent(new Event('change'));
  assert.match(doc.getElementById('category-operation-result').textContent,/not a property-transfer proof/);
  assert.match(doc.getElementById('category-operation-result').textContent,/symmetry-view/);
  assert.match(doc.getElementById('forgets-list').textContent,/non-symmetric transitions/);
});
test('negative and incomplete views keep separate scopes without operation controls',()=>{
  const doc=mount();select(doc,'check-category','RefutedAt');assert.equal(doc.getElementById('operations-panel').hidden,true);assert.match(doc.getElementById('status-explanation').textContent,/unselected declarations/);
  select(doc,'compose-path','UNKNOWN');assert.equal(doc.getElementById('view-status').textContent,'Incomplete');assert.equal(doc.getElementById('operations-panel').hidden,true);assert.match(doc.getElementById('status-explanation').textContent,/not performed: incomplete prior result/);
});
