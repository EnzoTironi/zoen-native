import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { parseWidgetSnapshot, snapshotRemaining } from './widget-snapshot.ts';
const base = { id:'a', appId:'app', template:'stat', title:'Live', accentHex:'#3D7A28', symbol:'leaf.fill', deepLink:'zoen://app/a' };
test('valid native templates, optional fields and null Codable optionals',()=> {
  for(const template of ['stat','progress','countdown','list','caption','ticket','photo']) {
    const snap=parseWidgetSnapshot({...base,template,codes:['SFO','LHR'],photo:'hike-tomales',value:null});
    assert.equal(snap?.template,template); assert.equal(snap?.value,undefined);
  }
});
test('native validation rejects unsafe identity, text, types and required template data',()=> {
  for(const patch of [{deepLink:'https://example.com'},{title:'https://example.com'},{accentHex:'red'},{symbol:'bad icon'},{template:'other'},{template:'photo',photo:'remote'},{template:'ticket',codes:['A']},{rows:[{text:'a',done:'yes'}]},{targetMs:1.5}]) assert.equal(parseWidgetSnapshot({...base,...patch}),null);
  assert.equal(parseWidgetSnapshot(null),null);
});
test('native caps and cleaning preserve source order and clamp gauges',()=> {
  const s=parseWidgetSnapshot({...base,title:'a'.repeat(60),detail:'https://x',art:'unrecognized',bars:[{label:'a'.repeat(20),value:-1},{label:'b',value:2},{label:'c',value:.5},{label:'d',value:1}],rows:[{text:'https://x',done:false},{text:'ok',done:true}],actions:[{tool:'invalid-tool',label:'No'},{tool:'feed',label:'a'.repeat(20)}],codes:['abcdef','B','C']});
  assert.equal(s?.title.length,48);assert.equal(s?.detail,undefined);assert.equal(s?.art,undefined);
  assert.deepEqual(s?.bars,[{label:'a'.repeat(16),value:0},{label:'b',value:1},{label:'c',value:.5}]);
  assert.deepEqual(s?.rows,[{text:'ok',done:true}]);assert.deepEqual(s?.actions,[{tool:'feed',label:'a'.repeat(14)}]);assert.deepEqual(s?.codes,['abcde','B']);
});
test('countdown uses whole native days hours minutes and never becomes negative',()=> {
  assert.deepEqual(snapshotRemaining(90061000,0),{days:1,hours:1,minutes:1});
  assert.deepEqual(snapshotRemaining(0,100),{days:0,hours:0,minutes:0});
});
