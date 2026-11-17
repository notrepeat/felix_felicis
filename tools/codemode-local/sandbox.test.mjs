import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { executeLocal, sandboxArguments } from './local.mjs';
import { mkdtemp, writeFile, rm, readdir, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { McpClient } from './client.mjs';

test('real SDK executes in local isolated Worker and only fixed callbacks',async()=>{
  const seen=[];
  const value=await executeLocal('async () => await felix.check({schema:"felix-mcp-v1"})',async(name,args)=>{seen.push({name,args});return {status:'admit'};});
  assert.deepEqual(value.result,{status:'admit'});assert.equal(value.metrics.calls,1);assert.equal(seen[0].name,'check');
});
test('generated code has no filesystem, process environment or outbound network',async()=>{
  process.env.FELIX_HOST_SECRET='HOST_SECRET_CANARY';
  const server=http.createServer((_,response)=>{response.end('HOST_CANARY');});
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  try{
    const port=server.address().port;
    const code=`async () => {
      let filesystem=false, network=false, environment=false;
      try { const fs=await import('node:fs'); filesystem=!!fs.readFileSync('/etc/passwd'); } catch {}
      try { network=(await fetch('http://127.0.0.1:${port}')).ok; } catch {}
      try { environment=typeof process !== 'undefined' && Object.keys(process.env).length > 0; } catch {}
      return {filesystem,network,environment};
    }`;
    const value=await executeLocal(code,async()=>{throw new Error('unexpected_call');});
    assert.deepEqual(value.result,{filesystem:false,network:false,environment:false});
  }finally{server.close();delete process.env.FELIX_HOST_SECRET;}
});
test('unknown capability absent and call budget enforced',async()=>{
  const unknown=await executeLocal('async () => await felix.apply({})',async()=>{throw new Error('must_not_call');});
  assert.ok(unknown.error);
  await assert.rejects(executeLocal('async () => {for(let i=0;i<17;i++)await felix.check({schema:"felix-mcp-v1"});}',async()=>({status:'admit'})),/call_limit/);
});
test('infinite loop is killed and reaped by OS process watchdog',async()=>{
  const before=await workerdPids();
  const start=performance.now();
  await assert.rejects(executeLocal('async () => {while(true){}}',async()=>{}, {timeoutMs:1000}),/execution_timeout/);
  assert.ok(performance.now()-start<5000);
  for(let attempt=0;attempt<20;attempt++){
    const after=await workerdPids();
    if(after.every(pid=>before.includes(pid)))return;
    await new Promise(resolve=>setTimeout(resolve,50));
  }
  assert.fail('workerd descendant survived timeout');
});
test('bounded serialized results and console output',async()=>{
  await assert.rejects(executeLocal('async () => "x".repeat(2000000)',async()=>{}),/output_limit/);
  await assert.rejects(executeLocal('async () => {console.log("x".repeat(2000000));return 1;}',async()=>{}),/output_limit/);
});

async function workerdPids(){
  const result=[];
  for(const entry of await readdir('/proc')){
    if(!/^\d+$/.test(entry))continue;
    try{const name=await readFile(`/proc/${entry}/comm`,'utf8');if(name.trim()==='workerd')result.push(entry);}catch{}
  }
  return result;
}
test('OS sandbox cannot read a real host canary even through trusted runtime fs',async()=>{
  const directory=await mkdtemp(join(tmpdir(),'felix-host-canary-'));
  const canary=join(directory,'secret');await writeFile(canary,'NOT_MOUNTED_CANARY');
  try{
    const args=await sandboxArguments();
    args.splice(-1,1,'-e',`const fs=require('node:fs'); process.stdout.write(JSON.stringify({exists:fs.existsSync(${JSON.stringify(canary)}),env:process.env.FELIX_HOST_SECRET??null}));`);
    const child=spawnSync('bwrap',args,{encoding:'utf8',env:{...process.env,FELIX_HOST_SECRET:'HOST_SECRET_CANARY'},timeout:5000});
    assert.equal(child.status,0);assert.deepEqual(JSON.parse(child.stdout),{exists:false,env:null});
  }finally{await rm(directory,{recursive:true,force:true});}
});
test('MCP close is idempotent after a signal exit',async()=>{
  const client=new McpClient(process.execPath,{root:'unused',manifest:'unused',invariants:'unused'});
  client.kill();await client.closed;await client.close();await client.close();
});
test('multibyte Unicode survives pipe chunk boundaries',async()=>{
  const value=await executeLocal('async () => "😀".repeat(90000)',async()=>{});
  assert.equal(value.result,'😀'.repeat(90000));
});
