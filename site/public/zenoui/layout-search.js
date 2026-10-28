// Closed, bounded presentation grammar. No DOM, I/O, clocks, or executable metadata.
import {searchUi,SEARCH_GRAMMAR,structuralDigest} from './ui-search.js';
import {validateAst,scoreAst,CHECKER} from './ui-grammar.js';
export {GRAMMAR,OBJECTIVE} from './ui-grammar.js';
const regions = ['state','commands','confirmation','verdict','metrics','comparison','ledger','record'];
const expert = ['state','metrics','comparison','ledger','record'];
const canonical = value => JSON.stringify(value, (_key, node) => node && typeof node === 'object' && !Array.isArray(node)
  ? Object.fromEntries(Object.keys(node).sort().map(k => [k,node[k]])) : node);
function exact(value, keys, error) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length !== keys.length || keys.some(k => !Object.hasOwn(value,k))) throw Error(error);
}
function label(value) {
  if (typeof value !== 'string' || !value.trim() || value.length > 300 || /[<>\p{Cc}\p{Cf}\p{Cs}\p{Co}\p{Zl}\p{Zp}\u034f\u115f\u1160\u17b4\u17b5\u180b-\u180f\u2065\u3164\ufe00-\ufe0f\uffa0\ufff0-\ufff8\u{E0000}-\u{E0FFF}]/u.test(value)) throw Error('UiProjectionLabel');
}
function integer(value) { return typeof value === 'string' && value.length <= 40 && /^(0|-?[1-9][0-9]*)$/.test(value); }
function inDomain(field, value) {
  return integer(value) && (field.domain.kind === 'int' ? BigInt(value) >= BigInt(field.domain.min) && BigInt(value) <= BigInt(field.domain.max) : field.options.some(o => o.value === value));
}
const V1=['format','primaryFields','detailFields','fixedFields','groupCommands','collapseExpert','compact','maxVisibleForms','maxExpandedExpertPanels','maxColumns','projections','quoteOverrides'];
export function validateRequirements(app, requirements) {
  const v2=requirements?.format==='zenoui/requirements/2';
  exact(requirements,v2?[...V1,'contextFields','groups']:V1,'UiInvalidRequirements');
  if (!['zenoui/requirements/1','zenoui/requirements/2'].includes(requirements.format)) throw Error('UiRequirementsVersion');
  if (app.fields.length > 64 || app.commands.length < 1 || app.commands.length > 16) throw Error('UiPresentationLimit');
  for (const key of ['groupCommands','collapseExpert','compact']) if (typeof requirements[key] !== 'boolean') throw Error('UiInvalidPreference');
  for (const [key,min,max] of [['maxVisibleForms',1,16],['maxExpandedExpertPanels',0,5],['maxColumns',1,2]])
    if (!Number.isInteger(requirements[key]) || requirements[key] < min || requirements[key] > max) throw Error('UiInvalidPresentationBound');
  const paths=[];
  for (const key of v2?['primaryFields','detailFields','fixedFields','contextFields']:['primaryFields','detailFields','fixedFields']) {
    if (!Array.isArray(requirements[key]) || requirements[key].length > 64) throw Error('UiPresentationLimit');
    for (const path of requirements[key]) {
      const f=app.fields.find(f=>f.path===path);
      if (!f || f.source==='state' || f.index===app.action || paths.includes(path)) throw Error('UiInvalidFieldBinding');
      if (key==='fixedFields' && !(f.source==='context' && f.domain.kind==='int' && f.domain.min===f.domain.max && f.initial===f.domain.min)) throw Error('UiInvalidFixedBinding');
      // Version 2: context comes from sign-in (or the preview's "Acting as"), never from a form.
      if (v2 && (key==='contextFields')!==(f.source==='context') && key!=='fixedFields') throw Error('UiInvalidContextBinding');
      paths.push(path);
    }
  }
  if (paths.length !== app.fields.filter(f=>f.source!=='state' && f.index!==app.action).length) throw Error('UiIncompleteFieldBindings');
  if (!Array.isArray(requirements.projections) || requirements.projections.length > 16) throw Error('UiPresentationLimit');
  const ids=new Set();
  for (const p of requirements.projections) {
    if (!['state-complement','accepted-decrease'].includes(p?.kind)) throw Error('UiProjectionGrammar');
    exact(p, p.kind==='state-complement' ? ['id','label','kind','fieldByCommand','total'] : ['id','label','kind','fieldByCommand'],'UiInvalidProjection');
    if (typeof p.id !== 'string' || !/^[a-z][a-z0-9-]{0,39}$/.test(p.id) || ids.has(p.id)) throw Error('UiProjectionIdentity');
    ids.add(p.id); label(p.label);
    // Version 1 declares a projection for every command; version 2 for the commands it applies to.
    if (v2) { const ids=Object.keys(p.fieldByCommand??{}); if (!ids.length || ids.some(id=>!app.commands.some(c=>c.id===id))) throw Error('UiProjectionCommands'); }
    else exact(p.fieldByCommand,app.commands.map(c=>c.id),'UiProjectionCommands');
    for (const path of Object.values(p.fieldByCommand)) {
      const f=app.fields.find(f=>f.path===path);
      if (!f || f.source!=='state' || f.domain.kind!=='int') throw Error('UiProjectionField');
      // Complement is a declared display projection, not a pricing or Authority claim.
      if (p.kind==='state-complement' && (!integer(p.total) || BigInt(p.total)<BigInt(f.domain.max) || BigInt(f.domain.min)<0n)) throw Error('UiProjectionTotal');
    }
  }
  const overrides=requirements.quoteOverrides;
  if (!overrides || typeof overrides!=='object' || Array.isArray(overrides) || Object.keys(overrides).length>64) throw Error('UiQuoteOverrides');
  for (const [path,value] of Object.entries(overrides)) {
    const f=app.fields.find(f=>f.path===path);
    if (!f || f.source!=='command' || f.index===app.action || f.domain.kind!=='int' || !requirements.detailFields.includes(path) || !inDomain(f,value)) throw Error('UiQuoteOverrideBinding');
  }
  if (!requirements.projections.some(p=>p.kind==='accepted-decrease') && Object.keys(overrides).length) throw Error('UiUnusedQuoteOverrides');
  if (v2) {
    if (!Array.isArray(requirements.groups) || requirements.groups.length > 8) throw Error('UiPresentationLimit');
    const grouped=new Set(),names=new Set();
    for (const g of requirements.groups) {
      exact(g,['id','commands'],'UiInvalidGroup');
      if (typeof g.id!=='string' || !/^[a-z][a-z0-9-]{0,23}$/.test(g.id) || names.has(g.id) || !Array.isArray(g.commands) || g.commands.length<2) throw Error('UiInvalidGroup');
      names.add(g.id);
      for (const id of g.commands) { if (!app.commands.some(c=>c.id===id) || grouped.has(id)) throw Error('UiInvalidGroup'); grouped.add(id); }
    }
    if (!requirements.collapseExpert || requirements.maxExpandedExpertPanels!==0) throw Error('UiExpertPanelsMustCollapse');
  }
  return true;
}
// Default requirements use every command input when native relevance is unavailable;
// context is separate from forms and the technical record stays collapsed.
export function deriveRequirements(app) {
  const inputs=app.fields.filter(f=>f.source==='command'&&f.index!==app.action);
  return {format:'zenoui/requirements/2',
    primaryFields:inputs.filter(f=>f.role!=='min-out').map(f=>f.path),detailFields:inputs.filter(f=>f.role==='min-out').map(f=>f.path),
    fixedFields:[],contextFields:app.fields.filter(f=>f.source==='context').map(f=>f.path),groups:[],
    groupCommands:false,collapseExpert:true,compact:true,maxVisibleForms:16,maxExpandedExpertPanels:0,maxColumns:2,projections:[],quoteOverrides:{}};
}
// Regions in tree order: the commands, the confirmation, the status and the five evidence panels.
function layoutOf(ast) {
  const layout=[];
  const visit=node=>{
    const id=node.kind==='commands'?'commands':node.kind==='confirmation'?'confirmation':node.kind==='status'?'verdict':node.kind==='evidence'?node.region:null;
    if(id)layout.push({id,slot:layout.length});
    for(const child of node.children??[])visit(child);
  };
  visit(ast);return layout;
}
export function synthesizePresentation(app, requirements) {
  validateRequirements(app,requirements);
  const result=searchUi(app,requirements);
  if(!result.ast)throw Error(`UiNoPresentation:${result.receipt.reason??'exhausted'}`);
  const checked=validateAst(app,requirements,result.ast,result.receipt.bounds);
  if(!checked.valid)throw Error('UiCompositionRefused');
  return {presentation:{ast:result.ast,requirements:structuredClone(requirements),summary:checked.summary,layout:layoutOf(result.ast)},searchReceipt:result.receipt};
}
// The page's own check before mounting, without replaying the search (which would stall the main
// thread): admission, the whole-tree checker, and selected-entry consistency with the tree.
// The noncryptographic digest is only a replay aid; generation replays the full search.
export function checkPresentation(app) {
  if(!['zenoui/app-description/2','zenoui/app-description/3'].includes(app.format)||!app.presentation||!app.searchReceipt)throw Error('UiPresentationVersion');
  const req=app.presentation.requirements,receipt=app.searchReceipt;
  validateRequirements(app,req);
  const result=validateAst(app,req,app.presentation.ast,receipt.bounds);
  if(!result.valid)throw Error('UiCompositionRefused');
  const layout=layoutOf(app.presentation.ast),score=scoreAst(app,req,app.presentation.ast,result.summary);
  const ordinal=receipt.selected?.ordinal,entry=receipt.candidateTrace?.[ordinal],digest=structuralDigest(canonical(app.presentation.ast));
  if(canonical(result.summary)!==canonical(app.presentation.summary)||canonical(layout)!==canonical(app.presentation.layout)||canonical(layout)!==canonical(app.layout)
    ||receipt.format!=='zenoui/ui-search/1'||receipt.grammar!==SEARCH_GRAMMAR||receipt.checker!==CHECKER||canonical(score)!==canonical(receipt.selectedScore)
    ||!Number.isSafeInteger(receipt.examined)||!Number.isSafeInteger(receipt.feasible)||receipt.feasible<1||receipt.examined<receipt.feasible
    ||!Number.isSafeInteger(ordinal)||ordinal<0||ordinal>=receipt.examined||!Array.isArray(receipt.candidateTrace)||receipt.candidateTrace.length!==receipt.examined
    ||receipt.selected.structuralDigest!==digest||!entry||entry.ordinal!==ordinal||entry.accepted!==true||entry.structuralDigest!==digest||canonical(entry.score)!==canonical(score))
    throw Error('UiPresentationReceiptMismatch');
  return true;
}
// Recheck every supplied object: an outer Object.freeze does not freeze nested data.
export function verifyPresentation(app) {
  if(app.format==='zenoui/app-description/1') {
    if(app.presentation||app.searchReceipt)throw Error('UiUnexpectedPresentation');
    return true;
  }
  if(!['zenoui/app-description/2','zenoui/app-description/3'].includes(app.format)||!app.presentation||!app.searchReceipt)throw Error('UiPresentationVersion');
  validateRequirements(app,app.presentation.requirements);
  const checked=validateAst(app,app.presentation.requirements,app.presentation.ast);
  if(!checked.valid)throw Error('UiCompositionRefused');
  const expected=synthesizePresentation(app,app.presentation.requirements);
  if(canonical(app.presentation)!==canonical(expected.presentation)||canonical(app.searchReceipt)!==canonical(expected.searchReceipt)||canonical(app.layout)!==canonical(expected.presentation.layout))throw Error('UiPresentationReceiptMismatch');
  return true;
}
