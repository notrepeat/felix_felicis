import { mkdtemp, mkdir, writeFile, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import assert from 'node:assert/strict';
import { McpClient } from './client.mjs';
import { executeLocal } from './local.mjs';

// Identical deterministic agent in both modes; repairs derive from tool data.
export async function correctionLoop(felix) {
  const context=await felix.context({schema:'felix-mcp-v1',node:'n/item',include_values:true});
  const words=context.invariants[0].formula.replace(/^\(|\)$/g,'').split(/\s+/);
  if(words[0]!=='Type')throw new Error('fixture_contract');
  const desired=`Type(${words[1].replace(/^@/,'')}, ${words[2]})`;
  const existing=context.atoms.find(atom=>atom.startsWith('Type('));
  const delta={schema:'felix-delta-v1',add:[desired],remove:[]};
  const rejected=await felix.propose({schema:'felix-mcp-v1',delta});
  const why=await felix.why({schema:'felix-mcp-v1',delta});
  if(rejected.status!=='reject'||why.diagnostics[0].class!=='Type')throw new Error('expected_rejection');
  const admitted=await felix.propose({schema:'felix-mcp-v1',delta:{...delta,remove:[existing]}});
  if(admitted.status!=='admit')throw new Error('expected_admission');
  return {initial:rejected.status,conflict:why.diagnostics[0].class,corrected:admitted.status,persisted:admitted.persisted};
}
export const CODE=`async () => (${correctionLoop.toString()})(felix)`;
const jsonBytes=value=>Buffer.byteLength(JSON.stringify(value));

async function main(){
  const binary=resolve(process.argv[2]??'../../target/debug/universo-mcp');
  const repetitions=Number(process.argv[3]??3);
  if(!Number.isInteger(repetitions)||repetitions<1||repetitions>20)throw new Error('invalid_repetitions');
  const temp=await mkdtemp(join(tmpdir(),'felix-codemode-'));
  const root=join(temp,'bundle');await mkdir(join(root,'n'),{recursive:true});
  const config={root,manifest:join(temp,'manifest.toml'),invariants:join(temp,'invariants.sx')};
  const original='---\ntype: a\ntitle: Synthetic Item\n---\nBODY_NOT_EXPOSED\n';
  await writeFile(join(root,'n/item.md'),original);
  await writeFile(config.manifest,'version = "code-mode/1"\n[nodos]\nsortes = ["a", "b", "Retirado", "Patrón"]\n[layout]\ndirectorios = ["n"]\n');
  await writeFile(config.invariants,'(Type @n/item b)\n');
  const samples=[];
  let expected;
  try{
    for(const mode of ['direct','code']){
      for(let repetition=0;repetition<repetitions;repetition++){
        const coldStart=performance.now();
        const client=new McpClient(binary,config);
        try{
          await client.initialize();
          const catalog=await client.request('tools/list',{});
          const startupMs=performance.now()-coldStart;
          for(const temperature of ['cold','warm']){
            let modelBytes=0;
            const before={input:client.bytesIn,output:client.bytesOut,calls:client.calls};
            const start=performance.now();
            const call=async(name,args)=>{
              const value=await client.tool(name,args);
              if(mode==='direct')modelBytes+=jsonBytes({name,arguments:args})+jsonBytes(value);
              return value;
            };
            let result;
            if(mode==='direct'){
              result=await correctionLoop(Object.fromEntries(['context','propose','check','why'].map(name=>[name,args=>call(name,args)])));
            }else{
              const executed=await executeLocal(CODE,call);
              if(executed.error)throw new Error('code_execution_failed');
              result=executed.result;modelBytes=jsonBytes({code:CODE})+jsonBytes(result);
            }
            const elapsed=performance.now()-start;
            if(expected)assert.deepEqual(result,expected);else expected=result;
            const calls=client.calls-before.calls;
            assert.equal(calls,4);
            samples.push({mode,temperature,repetition:repetition+1,elapsed_ms:elapsed+(temperature==='cold'?startupMs:0),mcp_startup_ms:temperature==='cold'?startupMs:0,outer_invocations:mode==='direct'?calls:1,internal_mcp_calls:calls,mcp_request_bytes:client.bytesIn-before.input,mcp_response_bytes:client.bytesOut-before.output,model_boundary_bytes:modelBytes,catalog_json_bytes:jsonBytes(catalog)});
            assert.equal(await readFile(join(root,'n/item.md'),'utf8'),original);
          }
        }finally{await client.close();}
      }
    }
    assert.deepEqual((await readdir(temp)).sort(),['bundle','invariants.sx','manifest.toml']);
    console.log(JSON.stringify({schema:'felix-codemode-comparison-v1',runtime:process.version,fixture:'synthetic-type-repair',agent:'deterministic-no-llm',warm_definition:'MCP process reused; each Code Mode execution creates fresh bubblewrap/workerd sandbox',bytes_definition:'UTF-8 JSON; model boundary excludes catalog; no tokenizer or model cost inferred',result:expected,samples},null,2));
  }finally{await rm(temp,{recursive:true,force:true});}
}
main().catch(()=>{console.error('comparison_failed');process.exitCode=1;});
