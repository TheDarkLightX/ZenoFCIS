import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {describe,checkPresentation,verifyPresentation} from '../public/zenoui/app-contract.js';
import {validateAst} from '../public/zenoui/ui-grammar.js';
import {editFixture,approvalFixture} from './idiom-fixtures.mjs';

const apps=[editFixture(),approvalFixture()].map(({raw,meta})=>describe(raw,meta));
function findIdiom(node){if(node.kind==='idiom')return node;return (node.children??[]).map(findIdiom).find(Boolean);}
function checkInvalidChildren(validate){
  for(const app of apps)for(let position=0;position<3;position++)for(const value of [null,undefined,0,'bad',{},[]]){
    const ast=structuredClone(app.presentation.ast);findIdiom(ast).children[position]=value;
    const result=validate(app,app.presentation.requirements,ast);
    assert.equal(result.valid,false);assert.ok(result.failures.some(f=>f.code==='node-schema'));
  }
}

test('malformed children in either idiom return structured invalid-tree results',()=>{
  checkInvalidChildren(validateAst);
  for(const app of apps){assert.equal(checkPresentation(app),true);assert.equal(verifyPresentation(app),true);}
});

test('fast admission and full replay refuse a null child with their typed error',()=>{
  for(const app of apps)for(let position=0;position<3;position++){
    const bad=structuredClone(app);findIdiom(bad.presentation.ast).children[position]=null;
    assert.throws(()=>checkPresentation(bad),/^Error: UiCompositionRefused$/);
    assert.throws(()=>verifyPresentation(bad),/^Error: UiCompositionRefused$/);
  }
});

test('malformed-child regression kills the unsafe topology helper',async()=>{
  const file=new URL('../public/zenoui/ui-grammar.js',import.meta.url),source=await readFile(file,'utf8');
  let code=source;
  for(const position of [0,1,2]){
    const safe=`node.children[${position}]?.kind`;assert.ok(code.includes(safe));code=code.replace(safe,`node.children[${position}].kind`);
  }
  code=code.replace(/(from\s+)(['"])(\.\/[^'"]+)\2/g,(_,prefix,quote,specifier)=>prefix+JSON.stringify(new URL(specifier,file).href));
  const mutant=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
  assert.throws(()=>checkInvalidChildren(mutant.validateAst),TypeError);
});
