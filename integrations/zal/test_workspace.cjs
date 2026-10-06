'use strict';
const vm = require('node:vm');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const root = path.resolve(__dirname, '../..');
// Generate owned fixtures through the actual domain; no provider or network.
const fixture = JSON.parse(execFileSync('python3', ['-c', `
import json
from pathlib import Path
from server import Workspace
from behavior import help_topic, explain
work = Workspace(Path('examples/order.zal').read_text(), Path.cwd())
seed = work.view()
def fixtures(model):
    topics = ['', 'rule:approve_order', 'rule:submit_order', 'event:approve', 'context:authorized', 'state:pending', 'and', 'grammar:&', 'grammar:default']
    return {'help': {topic: help_topic(model, topic) for topic in topics},
            'explain': explain(model, 'pending', 'approve', {'authorized': False}),
            'initial_explain': explain(model, model.initial, 'approve', {'authorized': False})}
seed_fixtures = fixtures(work.session.model)
work.discuss('Show the disclosed cancellation example', seed['model']['revision'], demo=True)
proposal = work.view()['proposal']
work.action('accept', {'base_revision': proposal['base_revision'], 'candidate_revision': proposal['revision']})
print(json.dumps({'seed': seed, 'accepted': work.view(), 'seed_data': seed_fixtures, 'accepted_data': fixtures(work.session.model)}))
`], {cwd: __dirname, encoding: 'utf8', env: {...process.env, PYTHONDONTWRITEBYTECODE: '1'}}));
// A small DOM double executes the actual script/event handlers, not browser defaults.
class Element {
  constructor(tag = 'div', owner = null) { this.ownerDocument = owner; this.tagName = tag.toUpperCase(); this.textContent = ''; this.value = ''; this.checked = false; this.children = []; this.dataset = {}; this.style = {}; this.open = false; this.hidden = false; this.disabled = false; this.attributes = {}; this.listeners = {}; this.classList = { toggle() {} }; this.isConnected = true; }
  append(...children) { this.children.push(...children); for (const child of children) child.parentElement = this; if (this.tagName === 'SELECT' && !this.value && this.children.length) this.value = this.children[0].value; }
  replaceChildren(...children) { this.children = []; this.append(...children); }
  setAttribute(name, value) { this.attributes[name] = String(value); if (name.startsWith('data-')) this.dataset[name.slice(5).replace(/-([a-z])/g, (_,c)=>c.toUpperCase())] = String(value); }
  addEventListener(name, fn) { this.listeners[name] = fn; }
  dispatch(name, event = {}) { event.preventDefault ||= () => { event.defaultPrevented = true; }; return this['on'+name]?.(event) ?? this.listeners[name]?.(event); }
  focus() { this.ownerDocument.activeElement = this; return this.dispatch('focus'); }
  showModal() { this.open = true; }
  close() { this.open = false; this.dispatch('close'); }
  querySelectorAll(selector) { const found=[]; for (const child of this.children) { if (child instanceof Element) { if (selector.startsWith('[')) {const [,key,value]=selector.match(/^\[([^=]+)="([^"]+)"\]$/)||[];if (key&&child.attributes[key]===value) found.push(child);} else if(child.tagName===selector.toUpperCase()) found.push(child); found.push(...child.querySelectorAll(selector)); } } return found; }
  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
}
function load(file, actualGraph = false) {
  const source = fs.readFileSync(file, 'utf8'), elements = new Map(), windowEvents={};
  const document = { activeElement: null, createElement: tag=>new Element(tag,document), createElementNS: (_,tag)=>new Element(tag,document), createTextNode: text=>({textContent:text}) };
  const get = id => { if (!elements.has(id)) {const tag=source.match(new RegExp('<([a-z]+)[^>]*\\bid="'+id+'"'))?.[1]||'div'; const e=new Element(tag,document);e.id=id;elements.set(id,e);} return elements.get(id); };
  document.getElementById = get; document.body=new Element('body',document);document.activeElement=document.body;
  get('editMode').value = 'english';
  const context = vm.createContext({document, window:{addEventListener(name,fn){windowEvents[name]=fn;}}, setTimeout(){}, console, fetch(){throw Error('No network in deterministic state test');} });
  let script = source.split('<script>')[1].split('</script>')[0];
  script = script.replace(/\npoll\(\);\s*$/, '\n');
  vm.runInContext(script, context, { filename:file });
  if (!actualGraph) vm.runInContext('drawGraph=()=>{};renderTrace=()=>{};',context);
  const run=code=>vm.runInContext(code,context);
  return {context,get,document,windowEvents,run, apply:value=>{context.next=JSON.parse(JSON.stringify(value));run('apply(next)');}};
}
const checks=[];
const check=(name,condition)=>{assert(condition,name);checks.push(name);};
(async()=>{
  const oldBase=fixture.seed.model.revision;
  const current=load(path.join(root,'integrations/zal/workspace.html'));
  current.apply(fixture.seed);
  check('workspace mode is visible',current.get('workspaceMode').textContent==='Workspace: isolated browser session');
  const nav=current.get('modelNav').children.map(e=>e.dataset.object);
  const model=fixture.seed.model;
  const expected=[...model.states.map(x=>'state:'+x),...model.events.map(x=>'event:'+x),...model.context.map(x=>'context:'+x),...model.rules.map(x=>'rule:'+x.id),...model.invariants.map(x=>'invariant:'+x[0]),'default','initial','frame'];
  check('navigation covers every declared object',JSON.stringify(nav)===JSON.stringify(expected));
  current.apply({...fixture.seed,version:1,selected:'event:approve'});
  check('event selection has matching English and symbols',current.get('englishRule').textContent==='The event is approve'&&current.get('symbolicRule').textContent==='event approve');
  current.apply({...fixture.seed,version:2,selected:'context:authorized'});
  check('context selection has matching English and symbols',current.get('englishRule').textContent==='The fresh Boolean observation is authorized'&&current.get('symbolicRule').textContent==='context authorized');
  current.apply({...fixture.seed,version:3,selected:'rule:approve_order'});
  current.get('editGuard').value='!authorized';current.get('editGuard').listeners.input();
  current.apply({...fixture.seed,version:4,selected:'state:pending'});
  check('selection changes preserve inline fields and their object/base',current.get('editGuard').value==='!authorized'&&current.run("ruleDirty&&ruleDraftId==='approve_order'&&ruleDraftBase===view.model.revision")&&!current.get('ruleEditor').hidden);
  current.run('openEditor()');current.get('sourceEditor').value='kept local draft';current.get('sourceEditor').oninput();
  current.apply({...fixture.accepted,version:5,selected:'rule:approve_order'});
  check('accepted revision preserves dirty full-source text and old base',current.get('sourceEditor').value==='kept local draft'&&current.run('editorBase')===oldBase&&current.run('editorDirty'));
  check('stale source draft is labeled and cannot submit',current.get('editRevision').textContent.startsWith('Stale draft:')&&current.get('checkEdit').disabled);
  current.get('editor').close();current.run('openEditor()');
  check('closing and reopening preserves stale source draft',current.get('sourceEditor').value==='kept local draft'&&current.run('editorBase')===oldBase);
  current.get('editMode').value='symbolic';current.get('editMode').onchange();
  check('dirty projection switch refuses without losing source',current.get('editMode').value==='english'&&current.get('sourceEditor').value==='kept local draft');
  current.run("api=async()=>{throw Error('Unexpected submission of stale draft')}");await current.get('checkEdit').onclick();
  check('stale check handler refuses before network action',current.run('editorDirty')&&current.get('checkEdit').disabled);
  check('accepted revision preserves and labels inline draft with disabled submit',current.get('editGuard').value==='!authorized'&&current.run('ruleDraftBase')===oldBase&&current.run('ruleDirty')&&current.get('ruleDraftStatus').textContent.startsWith('Stale transition draft:')&&current.get('proposeRule').disabled);
  await current.get('proposeRule').onclick();
  current.get('discardEdit').onclick();
  check('explicit source discard adopts accepted exact revision/text',!current.run('editorDirty')&&current.run('editorBase')===fixture.accepted.model.revision&&current.get('sourceEditor').value===fixture.accepted.model.english&&!current.get('checkEdit').disabled);
  current.get('discardRule').onclick();
  check('explicit inline discard adopts current selected rule/base',!current.run('ruleDirty')&&current.run('ruleDraftBase')===fixture.accepted.model.revision&&current.get('editGuard').value==='authorized'&&!current.get('proposeRule').disabled);
  // Clean editors may follow an external revision; dirty editors must keep their base.
  current.apply({...fixture.seed,version:6,selected:'rule:approve_order'});
  check('open clean full-source editor follows revision consistently',current.run('editorBase')===oldBase&&current.get('sourceEditor').value===fixture.seed.model.english&&!current.get('checkEdit').disabled);
  current.get('sourceEditor').value='first request';current.get('sourceEditor').oninput();
  let release;current.context.pending=new Promise(resolve=>{release=resolve;});current.run('api=()=>pending');const request=current.get('checkEdit').onclick();
  current.get('sourceEditor').value='later draft';current.get('sourceEditor').oninput();release({});await request;
  check('delayed source response does not clear later dirty text',current.run('editorDirty')&&current.get('sourceEditor').value==='later draft'&&current.get('editor').open);
  let releaseRule;current.context.pendingRule=new Promise(resolve=>{releaseRule=resolve;});current.run('api=()=>pendingRule');
  current.get('editGuard').value='!authorized';current.get('editGuard').listeners.input();const ruleRequest=current.get('proposeRule').onclick();
  current.get('editTarget').value='cancelled';current.get('editTarget').listeners.input();releaseRule({});await ruleRequest;
  check('delayed inline response does not clear later dirty fields',current.run('ruleDirty')&&current.get('editTarget').value==='cancelled');
  const bound=load(path.join(root,'integrations/zal/workspace.html'));bound.apply({...fixture.seed,selected:'rule:approve_order'});
  bound.get('editGuard').value='!authorized';bound.get('editGuard').listeners.input();bound.apply({...fixture.seed,version:1,selected:'rule:submit_order'});
  bound.run('api=async(route,data)=>{globalThis.submitted={route,data};return {}}');await bound.get('proposeRule').onclick();
  check('inline proposal remains bound to edited rule after another selection',bound.context.submitted.route==='propose'&&bound.context.submitted.data.revision===oldBase&&bound.context.submitted.data.surface.includes('rule approve_order: pending + approve [!authorized] -> accept approved;')&&bound.context.submitted.data.surface.includes('rule submit_order: draft + submit [true] -> accept pending;'));
  const race=load(path.join(root,'integrations/zal/workspace.html'));race.apply(fixture.seed);race.run('openEditor()');race.get('sourceEditor').value='pending old draft';race.get('sourceEditor').oninput();
  let releaseStale;race.context.pending=new Promise(resolve=>{releaseStale=resolve;});race.run('api=()=>pending');const staleResponse=race.get('checkEdit').onclick();race.apply(fixture.accepted);releaseStale({...fixture.seed,version:1});await staleResponse;
  check('older successful response cannot clear draft after external acceptance',race.run('editorDirty')&&race.run('editorBase')===oldBase&&race.get('sourceEditor').value==='pending old draft'&&race.get('editor').open&&race.get('checkEdit').disabled);
  const file=path.join(root,'integrations/zal/workspace.html');
  const clone=value=>JSON.parse(JSON.stringify(value));
  const settle=async()=>{await Promise.resolve();await Promise.resolve();};
  function ready(){
    const ui=load(file,true),calls=[];
    ui.context.apiStub=async(route,data)=>{
      calls.push({route,data:clone(data)});
      if(route==='help')return clone(fixture.seed_data.help[data.topic]);
      if(route==='explain')return clone(data.state==='pending'?fixture.seed_data.explain:fixture.seed_data.initial_explain);
      return {};
    };
    ui.run('api=(route,data)=>apiStub(route,data)');ui.apply(fixture.seed);return {ui,calls};
  }
  const {ui:local,calls:localCalls}=ready();
  check('current finite-check advisory states absent application invariants',local.get('checkAdvisories').textContent.includes('No application invariants declared'));
  await local.get('why').onclick();
  check('primary Explain routes locally with every explicit observation',localCalls.at(-1).route==='explain'&&localCalls.at(-1).data.revision===oldBase&&localCalls.at(-1).data.context.authorized===false&&!localCalls.some(call=>call.route==='discuss'||call.route==='connect'));
  check('local inspector renders exact tuple default frame guard and scope',local.get('explanation').textContent.includes('Default used: true')&&local.get('explanation').textContent.includes('Frame: control unchanged')&&local.get('explanation').textContent.includes('observed false')&&local.get('explanation').textContent.includes('no language model'));
  local.get('question').value='An explicit live discussion';await local.get('askLive').onclick();
  check('only explicit Ask Codex routes to non-demo discussion',localCalls.at(-1).route==='discuss'&&localCalls.at(-1).data.demo===false&&localCalls.at(-1).data.message==='An explicit live discussion');
  local.context.recorded=clone({pre:'pending',event:'approve',context:{authorized:false},outcome:fixture.seed_data.explain.outcome});
  local.run('trace=[recorded];traceInputs=[{event:recorded.event,context:{...recorded.context}}];scrub=1;renderTrace()');
  await local.get('whyRecorded').onclick();
  check('recorded Why uses exact row prestate and context',!local.get('whyRecorded').disabled&&localCalls.at(-1).data.state==='pending'&&local.get('explanation').textContent.startsWith('Recorded step 1'));
  local.get('restart').onclick();check('restart retires explanation and disables recorded Why',local.get('whyRecorded').disabled&&!local.get('explanation').textContent.startsWith('Recorded step'));
  await local.get('selectedHelp').onclick();
  check('visible object help is revision-bound and uses endpoint meaning',local.get('legend').open&&localCalls.at(-1).route==='help'&&localCalls.at(-1).data.topic===fixture.seed.selected&&local.get('legendText').textContent.includes(fixture.seed_data.help[fixture.seed.selected].meaning));
  check('help includes canonical source pairs and no-source-span limitation',local.get('legendText').textContent.includes(fixture.seed_data.help[fixture.seed.selected].entries[0].symbolic)&&local.get('legendText').textContent.includes('not arbitrary editor text or source spans'));
  local.get('closeLegend').onclick();
  check('help dismissal returns to invoking control through close handler',!local.get('legend').open&&local.document.activeElement===local.get('selectedHelp'));
  const navButton=local.get('modelNav').children.find(e=>e.dataset.object==='event:approve');
  await navButton.focus();
  check('keyboard focus requests exact typed-object help without opening dialog',localCalls.at(-1).data.topic==='event:approve'&&!local.get('legend').open&&local.get('helpPreview').textContent.includes('event:approve'));
  await navButton.dispatch('pointerenter',{pointerType:'mouse'});
  check('pointer hover exposes the same endpoint-derived meaning',local.get('helpPreview').textContent.includes(fixture.seed_data.help['event:approve'].meaning));
  const beforeTouch=localCalls.length;navButton.dispatch('pointerenter',{pointerType:'touch'});
  check('touch pointer does not depend on synthetic hover',localCalls.length===beforeTouch);
  await local.get('selectedHelp').dispatch('click',{pointerType:'touch'});
  check('touch click reaches visible canonical object help',local.get('legend').open&&localCalls.at(-1).data.topic===fixture.seed.selected);
  local.get('closeLegend').onclick();await navButton.focus();
  const keyEvent={key:'?',preventDefault(){this.prevented=true;}};local.windowEvents.keydown(keyEvent);await settle();
  check('question-mark keyboard shortcut uses focused typed ID',keyEvent.prevented&&local.get('legend').open&&localCalls.at(-1).data.topic==='event:approve');
  local.get('closeLegend').onclick();
  for(const id of ['question','helpTopic','traceEvent']){
    local.document.activeElement=local.get(id);const before=localCalls.length;
    local.windowEvents.keydown({key:'?',preventDefault(){throw Error('Typed question mark was hijacked');}});
    check('question-mark shortcut leaves '+id+' editing untouched',localCalls.length===before&&!local.get('legend').open);
  }
  const graphButton=local.get('graph').querySelector('[data-focus="rule:approve_order"]');
  await graphButton.focus();
  check('graph focus help keeps full canonical rule identity',localCalls.at(-1).data.topic==='rule:approve_order'&&graphButton.attributes['aria-describedby']==='helpPreview');
  local.context.selectedFromGraph=null;local.run('choose=id=>{selectedFromGraph=id}');let prevented=false;
  graphButton.onkeydown({key:' ',preventDefault(){prevented=true;}});
  check('graph keyboard selection handler retains space activation',prevented&&local.context.selectedFromGraph==='rule:approve_order');
  await local.run("openHelp('and')");
  check('operator aliases accept server canonical normalization',local.get('helpTitle').textContent===fixture.seed_data.help.and.title&&local.get('helpRevision').textContent.includes('Topic: &'));
  const topicValues=local.get('helpTopics').children.map(e=>e.value);
  check('topic chooser keeps typed IDs exact and disambiguates grammar',topicValues.includes('rule:approve_order')&&topicValues.includes('grammar:rule')&&topicValues.includes('grammar:&'));
  local.get('helpTopic').value='grammar:&';await local.get('helpForm').onsubmit({preventDefault(){}});
  check('help form submits explicit topic without inferred source mapping',localCalls.at(-1).data.topic==='grammar:&');
  local.get('helpTopics').value='grammar:default';await local.get('helpTopics').onchange();
  check('topic chooser routes grammar topic through metadata endpoint',local.get('helpTitle').textContent===fixture.seed_data.help['grammar:default'].title);
  local.get('closeLegend').onclick();local.run('openEditor()');local.get('sourceEditor').value='Unsaved ? words';local.get('sourceEditor').oninput();
  const sourceBefore={text:local.get('sourceEditor').value,base:local.run('editorBase'),generation:local.run('editGeneration')};
  local.get('editor').close();await local.get('selectedHelp').onclick();local.get('closeLegend').onclick();
  check('help preserves dirty source draft base text and edit generation',local.run('editorDirty')&&local.run('editorBase')===sourceBefore.base&&local.run('editGeneration')===sourceBefore.generation&&local.get('sourceEditor').value===sourceBefore.text);
  // Independently controlled responses test latest-request and exact-revision binding.
  const {ui:helpRace}=ready();let pendingHelp=[];helpRace.context.apiStub=(route,data)=>new Promise(resolve=>pendingHelp.push({route,data,resolve}));
  const firstHelp=helpRace.run("openHelp('rule:approve_order')"),secondHelp=helpRace.run("openHelp('rule:submit_order')");
  pendingHelp[1].resolve(clone(fixture.seed_data.help['rule:submit_order']));await secondHelp;
  pendingHelp[0].resolve(clone(fixture.seed_data.help['rule:approve_order']));await firstHelp;
  check('newer topic response wins over delayed older help',helpRace.get('helpTitle').textContent.includes('rule:submit_order'));
  const selectedPending=helpRace.run("loadHelp('rule:approve_order')");
  helpRace.apply({...fixture.seed,version:1,selected:'rule:submit_order'});pendingHelp.at(-1).resolve(clone(fixture.seed_data.help['rule:approve_order']));await selectedPending;
  check('same-revision changed selection refuses old help overwrite',helpRace.get('legendText').textContent.includes('Selection or revision changed'));
  const revisedPending=helpRace.run("loadHelp('rule:approve_order')");helpRace.apply({...fixture.accepted,version:2});pendingHelp.at(-1).resolve(clone(fixture.seed_data.help['rule:approve_order']));await revisedPending;
  check('accepted revision refuses old help and clears topic choices',helpRace.get('legendText').textContent.includes('Selection or revision changed')&&helpRace.get('helpRevision').textContent===''&&helpRace.get('helpTopics').children.length===0);
  const {ui:editHelp}=ready();let resolveDraft;editHelp.context.apiStub=()=>new Promise(resolve=>{resolveDraft=resolve;});const oldQuery=editHelp.run("openHelp('rule:approve_order')");
  editHelp.get('helpTopic').value='my next topic';editHelp.get('helpTopic').dispatch('input');resolveDraft(clone(fixture.seed_data.help['rule:approve_order']));await oldQuery;
  check('typing a new help query prevents old response overwrite',editHelp.get('legendText').textContent==='Topic changed. Read help to inspect it.'&&editHelp.get('helpTopic').value==='my next topic');
  const {ui:closedHelp}=ready();let resolveClosed;closedHelp.context.apiStub=()=>new Promise(resolve=>{resolveClosed=resolve;});const closedQuery=closedHelp.run("openHelp('rule:approve_order')");closedHelp.get('closeLegend').onclick();resolveClosed(clone(fixture.seed_data.help['rule:approve_order']));await closedQuery;
  check('dismissed help cannot be reopened by its pending response',!closedHelp.get('legend').open&&closedHelp.get('helpTitle').textContent==='Language help');
  const {ui:previewRace}=ready();let previews=[];previewRace.context.apiStub=(route,data)=>new Promise(resolve=>previews.push({data,resolve}));
  const previewA=previewRace.run("previewHelp('event:approve')"),previewB=previewRace.run("previewHelp('context:authorized')");previews[1].resolve(clone(fixture.seed_data.help['context:authorized']));await previewB;previews[0].resolve(clone(fixture.seed_data.help['event:approve']));await previewA;
  check('newest focus or hover preview wins delayed topic race',previewRace.get('helpPreview').textContent.includes('context:authorized')&&!previewRace.get('helpPreview').textContent.includes('Meaning of event:approve'));
  const previewRevision=previewRace.run("previewHelp('event:approve')");previewRace.apply(fixture.accepted);previews.at(-1).resolve(clone(fixture.seed_data.help['event:approve']));await previewRevision;
  check('preview rejects delayed old-revision result',!previewRace.get('helpPreview').textContent.includes('Meaning of event:approve'));
  const {ui:identity}=ready();identity.context.apiStub=async()=>clone(fixture.seed_data.help['rule:submit_order']);await identity.run("openHelp('rule:approve_order')");
  check('typed object identity mismatch is refused',identity.get('legendText').textContent.includes('changed the requested object identity'));
  identity.context.apiStub=async()=>clone(fixture.accepted_data.help['rule:approve_order']);await identity.run("loadHelp('rule:approve_order')");
  check('mismatched help response revision is independently refused',identity.get('legendText').textContent.includes('does not match the current model'));
  identity.context.apiStub=async()=>{throw Error('Ambiguous help topic; use a typed object ID');};await identity.run("loadHelp('approve')");
  check('ambiguous help refusal stays explicit without guessed meaning',identity.get('legendText').textContent.startsWith('Help refused: Ambiguous'));
  const {ui:explainRace}=ready();let resolveExplain;explainRace.context.apiStub=()=>new Promise(resolve=>{resolveExplain=resolve;});const oldExplain=explainRace.get('why').onclick();
  const fact=explainRace.get('factInputs').querySelector('input');fact.checked=true;fact.dispatch('input');resolveExplain(clone(fixture.seed_data.initial_explain));await oldExplain;
  check('changed observation retires pending decision explanation',!explainRace.get('explanation').textContent.includes('Default used:'));
  fact.checked=false;const revisedExplain=explainRace.get('why').onclick();explainRace.apply(fixture.accepted);resolveExplain(clone(fixture.seed_data.initial_explain));await revisedExplain;
  check('new model revision retires pending decision explanation',!explainRace.get('explanation').textContent.includes('Default used:'));
  const {ui:wrongInput}=ready();wrongInput.context.apiStub=async()=>clone(fixture.seed_data.explain);await wrongInput.get('why').onclick();
  check('explanation response must match exact state event and context',wrongInput.get('explanation').textContent.includes('does not match the requested input and revision'));
  const {ui:missingFact}=ready();missingFact.get('factInputs').replaceChildren();let missingRequest;
  missingFact.context.apiStub=async(route,data)=>{missingRequest=clone(data);throw Error('Context needs exactly every declared Boolean fact');};await missingFact.get('why').onclick();
  check('missing observation is never invented for explanation',Object.keys(missingRequest.context).length===0&&missingFact.get('explanation').textContent.includes('Context needs exactly every declared Boolean fact'));
  const {ui:lateFailure}=ready();let failHelp;lateFailure.context.apiStub=()=>new Promise((resolve,reject)=>{failHelp=reject;});const failingHelp=lateFailure.run("openHelp('rule:approve_order')");
  lateFailure.apply({...fixture.seed,version:1,selected:'event:approve'});failHelp(Error('obsolete failure'));await failingHelp;
  check('stale help errors cannot overwrite newer selection status',!lateFailure.get('legendText').textContent.includes('obsolete failure')&&lateFailure.get('legendText').textContent.includes('Selection or revision changed'));
  const {ui:pollHelp}=ready();await pollHelp.run("openHelp('and')");pollHelp.get('helpTopic').value='draft topic';pollHelp.get('helpTopic').dispatch('input');pollHelp.apply({...fixture.seed,version:1});
  check('unchanged-revision polling preserves pending help query',pollHelp.get('helpTopic').value==='draft topic'&&pollHelp.get('legendText').textContent==='Topic changed. Read help to inspect it.');
  pollHelp.context.apiStub=async()=>({...clone(fixture.seed_data.help.and),schema:'unknown'});await pollHelp.run("loadHelp('and')");
  check('unknown help schema is refused',pollHelp.get('legendText').textContent.includes('does not match the current model'));
  const {ui:latestExplain}=ready();const pendingExplanations=[];latestExplain.context.apiStub=()=>new Promise(resolve=>pendingExplanations.push(resolve));
  const explanationA=latestExplain.get('why').onclick(),explanationB=latestExplain.get('why').onclick();pendingExplanations[1](clone(fixture.seed_data.initial_explain));await explanationB;const newestExplanation=latestExplain.get('explanation').textContent;pendingExplanations[0]({...clone(fixture.seed_data.initial_explain),method:'obsolete response'});await explanationA;
  check('latest explanation wins repeated asynchronous request race',latestExplain.get('explanation').textContent===newestExplanation&&!newestExplanation.includes('obsolete response'));
  const waiting=load(file,true);await waiting.get('why').onclick();
  check('explanation before initial load waits without a request',waiting.get('explanation').textContent.includes('Wait for the current workspace'));
  waiting.context.coldStartCalls=0;waiting.run("api=async()=>{coldStartCalls++;throw Error('Unexpected cold-start API call')}");await waiting.get('selectedHelp').onclick();
  check('object help before initial load waits without any API or model call',waiting.context.coldStartCalls===0&&!waiting.get('legend').open&&waiting.get('message').textContent==='Wait for the current workspace to load before requesting object help.');
  identity.context.apiStub=async()=>clone(fixture.seed_data.help['rule:approve_order']);await identity.run("loadHelp('default')");
  check('special canonical object help retains exact identity',identity.get('legendText').textContent.includes('changed the requested object identity'));
  const report={schema:'zal/deterministic-workspace-check/1',checks,count:checks.length,limitations:['Minimal DOM doubles execute actual UI state and event handlers. They do not establish native rendering, focus behavior, touch/keyboard defaults, layout, HTTP transport or CSP.']};
  console.log(JSON.stringify(report,null,2));
})().catch(error=>{console.error(error);process.exitCode=1});
