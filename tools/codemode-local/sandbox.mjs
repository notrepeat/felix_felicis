// Trusted local runtime. Runs inside bubblewrap, never receives bundle paths.
import { Miniflare } from 'miniflare';
import { readFile } from 'node:fs/promises';
import readline from 'node:readline';
const pending = new Map();
let serial = 0;
let started = false;
const lines = readline.createInterface({input:process.stdin});
lines.on('line', async line => {
  const message = JSON.parse(line);
  if (message.kind === 'reply') { pending.get(message.id)?.(message.value); pending.delete(message.id); return; }
  if (started || message.kind !== 'execute') return;
  started = true;
  const mf = new Miniflare({modules:true, script:await readFile('/app/worker.bundle.mjs','utf8'), compatibilityDate:'2026-07-01', workerLoaders:{LOADER:{}}, serviceBindings:{BRIDGE:async request=>{
    const call = await request.json();
    const id = ++serial;
    const response = new Promise(resolve=>pending.set(id,resolve));
    process.stdout.write(JSON.stringify({kind:'call',id,...call})+'\n');
    return Response.json(await response);
  }}});
  try {
    const response = await mf.dispatchFetch('http://local/execute',{method:'POST',body:JSON.stringify({code:message.code})});
    const result = await response.json();
    process.stdout.write(JSON.stringify({kind:'result',value:result})+'\n');
  } catch { process.stdout.write(JSON.stringify({kind:'result',value:{error:'execution_failed'}})+'\n'); }
  finally { await mf.dispose(); lines.close(); process.stdin.destroy(); }
});
