// Operator entry point; configuration paths never enter the generated program.
import { open } from 'node:fs/promises';
import { constants } from 'node:fs';
import { resolve } from 'node:path';
import { McpClient } from './client.mjs';
import { executeLocal, LIMITS } from './local.mjs';
let client;
try {
  const [binary,root,manifest,invariants,program,...extra]=process.argv.slice(2);
  if(!program||extra.length)throw new Error('usage');
  const file=await open(program,constants.O_RDONLY|constants.O_NOFOLLOW|constants.O_NONBLOCK);
  let code;
  try {
    if(!(await file.stat()).isFile())throw new Error('code_not_regular');
    const bytes=Buffer.alloc(LIMITS.codeBytes+1);
    let count=0;
    while(count<bytes.length){const read=await file.read(bytes,count,bytes.length-count,null);if(!read.bytesRead)break;count+=read.bytesRead;}
    if(count>LIMITS.codeBytes)throw new Error('code_limit');
    code=new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(0,count));
  }finally{await file.close();}
  client=new McpClient(resolve(binary),{root:resolve(root),manifest:resolve(manifest),invariants:resolve(invariants)});
  await client.initialize();
  const result=await executeLocal(code,(name,args)=>client.tool(name,args));
  console.log(JSON.stringify(result));
  if(result.error)process.exitCode=1;
}catch{console.error('local_codemode_failed');process.exitCode=1;}
finally{await client?.close();}
