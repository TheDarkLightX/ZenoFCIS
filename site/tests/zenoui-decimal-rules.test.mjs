// The rules a decimal input parser must meet before the UI accepts decimal
// amounts. A decimal field declares a scale S and a domain [min, max] in base
// units, the integers the core decides on: the text "1.25" at scale 2 is 125
// base units. The rules:
//
// 1. Exact. The result is the base-unit integer as a decimal string, computed
//    with BigInt or string arithmetic, never Number or parseFloat, so every
//    value up to i128 keeps every digit.
// 2. No silent rounding. More than S fraction digits is refused
//    (DecimalScale), even when the extra digits are zeros; nothing is
//    rounded, truncated or padded past S.
// 3. One spelling. ASCII digits, at most one '.', with at least one digit on
//    each side of it; '-' only first, and only where the domain allows a
//    negative value. Refused (DecimalSyntax): exponents, '+', other bases,
//    whitespace anywhere, grouping separators of any locale, a decimal comma,
//    other digit systems, the Unicode minus, leading zeros before the units
//    digit, negative zero, and invisible characters.
// 4. Bounded. Text longer than 41 characters is refused (DecimalLength)
//    before any arithmetic; the base-unit value must lie in [min, max]
//    (DecimalRange), whatever its sign.
// 5. Round trip. format(units, S) writes the plain form with exactly S
//    fraction digits and no grouping, and parse(format(u)) == u for every u
//    in the domain. Grouping is for display only, never for editable text.
//
// A UI parser adopts these rules by passing its own functions to
// checkDecimalParser; this file checks the reference implementation below.
import assert from 'node:assert/strict';
import {test} from 'node:test';

const SYNTAX=/^-?(0|[1-9][0-9]*)(\.[0-9]+)?$/;
// Reference parser: text, scale and inclusive base-unit bounds (decimal strings).
export function parseDecimal(text,{scale,min,max}) {
  if(typeof text!=='string')return {ok:false,error:'DecimalSyntax'};
  if(text.length>41)return {ok:false,error:'DecimalLength'};
  if(!SYNTAX.test(text)||/^-0(\.0+)?$/.test(text))return {ok:false,error:'DecimalSyntax'};
  const negative=text.startsWith('-'),[whole,fraction='']=text.slice(negative?1:0).split('.');
  if(fraction.length>scale)return {ok:false,error:'DecimalScale'};
  const magnitude=BigInt(whole+fraction.padEnd(scale,'0')),units=negative?-magnitude:magnitude;
  if(units<BigInt(min)||units>BigInt(max))return {ok:false,error:'DecimalRange'};
  return {ok:true,units:units.toString()};
}
// Reference formatter: the plain form with exactly `scale` fraction digits.
export function formatDecimal(units,scale) {
  const value=BigInt(units),negative=value<0n,digits=(negative?-value:value).toString().padStart(scale+1,'0');
  const whole=digits.slice(0,digits.length-scale),fraction=digits.slice(digits.length-scale);
  return (negative?'-':'')+whole+(scale?'.'+fraction:'');
}

const I128_MIN=(-(1n<<127n)).toString(),I128_MAX=((1n<<127n)-1n).toString();
// [text, scale, min, max, expected base units or error name]
export const DECIMAL_CASES=[
  // Exact values, including past Number's 2^53 and at the i128 bounds.
  ['0',2,'0','100000','0'],['0.00',2,'0','100000','0'],['1',2,'0','100000','100'],['1.5',2,'0','100000','150'],
  ['1.25',2,'0','100000','125'],['0.01',2,'0','100000','1'],['0.1',18,'0',I128_MAX,'100000000000000000'],
  ['90071992547409.93',2,'0',I128_MAX,'9007199254740993'],['0.3',1,'0','10','3'],
  ['1701411834604692317316873037.15884105727',11,I128_MIN,I128_MAX,I128_MAX],
  ['-1701411834604692317316873037.15884105728',11,I128_MIN,I128_MAX,I128_MIN],
  ['-1.5',2,'-1000','1000','-150'],['-0.01',2,'-1000','1000','-1'],
  // No silent rounding: extra fraction digits are refused, zeros included.
  ['1.005',2,'0','100000','DecimalScale'],['1.250',2,'0','100000','DecimalScale'],['0.000000000000000000001',18,'0',I128_MAX,'DecimalScale'],
  ['1.5',0,'0','100','DecimalScale'],['0.1',0,'0','100','DecimalScale'],
  // Range, in base units, including a sign the domain does not allow.
  ['1000.01',2,'0','100000','DecimalRange'],['-0.01',2,'0','100000','DecimalRange'],['1701411834604692317316873037.15884105728',11,I128_MIN,I128_MAX,'DecimalRange'],
  // Length before arithmetic.
  ['1'.repeat(42),0,'0',I128_MAX,'DecimalLength'],['0.'+'0'.repeat(40),40,'0','1','DecimalLength'],
  // One spelling.
  ...['1e3','1E3','1e-2','0x10','Infinity','NaN','+1','--1','-','.','.5','5.','1..5','1.2.3','01','00.5','-0','-0.0','-0.00',
      ' 1','1 ','1\t',' 1','1 ','1 000','1,000','1.000,5','1\'000','1_000','1,5','−1','١','１','​1','1‮',''
     ].map(text=>[text,2,'-100000000','100000000','DecimalSyntax']),
];

export function checkDecimalParser({parseDecimal:parse,formatDecimal:format}) {
  for(const [text,scale,min,max,expected] of DECIMAL_CASES){
    const result=parse(text,{scale,min,max});
    const want=/^Decimal/.test(expected)?{ok:false,error:expected}:{ok:true,units:expected};
    assert.deepEqual(result,want,`parse ${JSON.stringify(text)} at scale ${scale}`);
  }
  // Round trip over a fixed-seed sample, including the bounds.
  let state=0x2026n;const next=()=>{state=(state*6364136223846793005n+1442695040888963407n)&((1n<<64n)-1n);return state;};
  for(const [scale,min,max] of [[0,'0','1000'],[2,'-100000','100000'],[6,'0',(1n<<64n).toString()],[18,I128_MIN,I128_MAX]]){
    const low=BigInt(min),span=BigInt(max)-low+1n;
    for(let i=0;i<500;i++){
      const units=(i<2?[low,BigInt(max)][i]:low+((next()<<64n)|next())%span).toString();
      const text=format(units,scale);
      assert.deepEqual(parse(text,{scale,min,max}),{ok:true,units},`round trip of ${units} at scale ${scale} via ${text}`);
      assert.ok(!/[,_\s']/.test(text),'editable text is never grouped');
    }
  }
}

test('the_reference_parser_meets_the_decimal_rules',()=>checkDecimalParser({parseDecimal,formatDecimal}));
test('a_rounding_parser_fails_the_rules',()=>{
  // A planted parser that rounds through Number, as parseFloat-based inputs do.
  const rounding=(text,{scale,min,max})=>{const n=Number(text);if(!Number.isFinite(n))return {ok:false,error:'DecimalSyntax'};const units=BigInt(Math.round(n*10**scale));return units<BigInt(min)||units>BigInt(max)?{ok:false,error:'DecimalRange'}:{ok:true,units:units.toString()};};
  assert.throws(()=>checkDecimalParser({parseDecimal:rounding,formatDecimal}),/parse/);
});
