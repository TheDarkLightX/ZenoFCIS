import assert from 'node:assert/strict';
import {test} from 'node:test';
import {moduleLoader,ORIGIN} from './browser-fixture.mjs';

test('recursive UI imports share one module per context and refuse unserved routes',async()=>{
  const seen=new Map();
  const first=moduleLoader({},(url,source)=>{seen.set(url,(seen.get(url)??0)+1);return source;});
  const appUrl=`${ORIGIN}/ZenoFCIS/zenoui/app.js`,contractUrl=`${ORIGIN}/ZenoFCIS/zenoui/app-contract.js`;
  const [app,contract,again]=await Promise.all([first.load(appUrl),first.load(contractUrl),first.load(appUrl)]);
  assert.equal(app,again);assert.equal(app.status,'linked');assert.equal(contract.status,'linked');
  assert.equal(seen.get(contractUrl),1,'shared dependency constructed once');
  assert.equal(seen.get(`${ORIGIN}/ZenoFCIS/zenoui/layout-search.js`),1);
  const second=moduleLoader({});const other=await second.load(contractUrl);
  assert.notEqual(other,contract,'each VM context owns its module');assert.notEqual(second.context,first.context);
  await assert.rejects(first.load(`${ORIGIN}/public/zenoui/app.js`),/Unserved asset/);
  assert.equal(await first.load(appUrl),app,'a refused route does not poison valid modules');
  assert.equal((await first.evaluate(contractUrl)).namespace.REGIONS.length,8);
});
