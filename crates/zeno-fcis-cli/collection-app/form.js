'use strict';
const byId = id => document.getElementById(id);
let secret, head, prepared, retained;
const keyBytes = text => Uint8Array.from(text.match(/../g), v => parseInt(v, 16));
async function call(route, body) {
  const response = await fetch(route, {method: body ? 'POST' : 'GET', headers: {'Authorization': `Bearer ${secret}`}, body});
  const text = await response.text();
  if (!response.ok) throw new Error(`${response.status} ${text}`);
  return JSON.parse(text);
}
function show(value) { byId('result').textContent = JSON.stringify(value, null, 2); }
function request(command) {
  const encoded = new TextEncoder().encode(JSON.stringify(command));
  const bytes = new Uint8Array(73 + encoded.length);
  bytes[0] = 1;
  bytes.set(keyBytes(head.contract), 1);
  bytes.set(crypto.getRandomValues(new Uint8Array(32)), 33);
  new DataView(bytes.buffer).setBigUint64(65, BigInt(head.revision));
  bytes.set(encoded, 73);
  return bytes;
}
byId('sign-in').onsubmit = async event => {
  event.preventDefault();
  try {
    secret = byId('credential').value;
    if (!/^[0-9a-f]{64}$/.test(secret)) throw new Error('Credential needs 64 lowercase hexadecimal digits.');
    head = await call('/state');
    byId('fields').replaceChildren();
    for (const field of head.interface.fields.filter(f => f.source === 'command')) {
      const label = document.createElement('label');
      label.textContent = `${field.name} (${field.domain.kind})`;
      const input = document.createElement('input');
      input.name = field.name;
      input.autocomplete = 'off';
      input.spellcheck = false;
      input.required = true;
      if (field.domain.kind === 'key') { input.pattern = '[0-9a-f]{64}'; input.maxLength = 64; input.placeholder = '64 lowercase hexadecimal digits'; }
      else input.value = field.domain.min ?? String(field.domain.variants?.[0] ?? 0);
      label.append(input);
      byId('fields').append(label);
    }
    byId('command').hidden = false;
    byId('status').textContent = `Revision ${head.revision}. Context is configured by the server.`;
    show(head);
  } catch (error) { byId('status').textContent = error.message; }
};
byId('command').oninput = () => { prepared = undefined; byId('commit').disabled = true; };
byId('command').onsubmit = async event => {
  event.preventDefault();
  try {
    if (retained) throw new Error('Resolve the retained command before preparing another one.');
    head = await call('/state');
    const values = Array.from(byId('fields').querySelectorAll('input'), input => input.value);
    prepared = request(values);
    const result = await call('/preview', prepared);
    show(result);
    byId('commit').disabled = result.reply.class === 'Reject';
  } catch (error) { prepared = undefined; byId('commit').disabled = true; byId('status').textContent = error.message; }
};
async function commit() {
  try {
    const result = await call('/command', retained);
    show(result);
    retained = undefined;
    prepared = undefined;
    byId('retry').disabled = true;
    byId('commit').disabled = true;
    byId('status').textContent = `Revision ${result.revision}.`;
  } catch (error) {
    byId('status').textContent = `${error.message}. The exact command is retained for retry in this page.`;
    byId('retry').disabled = false;
  }
}
byId('commit').onclick = async () => {
  if (!prepared || retained) return;
  retained = prepared;
  byId('commit').disabled = true;
  await commit();
};
byId('retry').onclick = async () => { if (retained) await commit(); };
