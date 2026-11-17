import { spawn } from 'node:child_process';
import { StringDecoder } from 'node:string_decoder';
import { fileURLToPath } from 'node:url';
import { realpath } from 'node:fs/promises';
import { resolve } from 'node:path';
const directory = fileURLToPath(new URL('.',import.meta.url));
export const LIMITS = Object.freeze({codeBytes:64*1024, argumentBytes:64*1024, outputBytes:1024*1024, calls:16, timeoutMs:12_000});
const allowed = new Set(['context','propose','check','why']);

// Shared with OS-boundary tests; no bundle path enters these mounts.
export async function sandboxArguments() {
  const runtime = await realpath(process.execPath);
  const dependencies = await realpath(resolve(process.env.FELIX_CODEMODE_CACHE ?? resolve(directory,'../../target/codemode-local'),'node_modules'));
  const args=['--unshare-all','--die-with-parent','--new-session','--clearenv'];
  for(const path of ['/usr','/bin','/lib','/lib64']) args.push('--ro-bind',path,path);
  args.push('--dir','/app','--ro-bind',resolve(directory,'sandbox.mjs'),'/app/sandbox.mjs','--ro-bind',resolve(directory,'worker.bundle.mjs'),'/app/worker.bundle.mjs','--ro-bind',dependencies,'/app/node_modules','--ro-bind',runtime,'/node','--proc','/proc','--dev','/dev','--tmpfs','/tmp','--setenv','PATH','/usr/bin:/bin','--setenv','HOME','/tmp','--chdir','/app','/node','sandbox.mjs');
  return args;
}

// No Node eval/vm. Untrusted JS executes only in the SDK Dynamic Worker,
// inside a separate local workerd process and OS namespace sandbox.
export async function executeLocal(code, callTool, overrides = {}) {
  const limits = {...LIMITS,...overrides};
  if (typeof code !== 'string' || Buffer.byteLength(code)>limits.codeBytes) throw new Error('code_limit');
  if (process.platform !== 'linux') throw new Error('local_sandbox_requires_linux');
  const args=await sandboxArguments();
  const child=spawn('bwrap',args,{stdio:['pipe','pipe','pipe'],detached:true,env:{PATH:process.env.PATH}});
  return new Promise((resolve,reject)=>{
    const decoder=new StringDecoder('utf8');
    let buffer='', bytes=0, calls=0, busy=false, outcome, failure, finished=false;
    const stop=(error,value)=>{
      if(finished)return;
      finished=true;failure=error;outcome=value;
      clearTimeout(timer);
      // Killing namespace init also terminates its descendants; wait for close.
      try{process.kill(-child.pid,'SIGKILL');}catch{child.kill('SIGKILL');}
    };
    const timer=setTimeout(()=>stop(new Error('execution_timeout')),limits.timeoutMs);
    child.on('error',error=>{clearTimeout(timer);reject(new Error('sandbox_unavailable',{cause:error}));});
    child.on('close',()=>{clearTimeout(timer);if(failure)reject(failure);else if(outcome)resolve({...outcome,metrics:{calls,output_bytes:bytes}});else reject(new Error('sandbox_terminated'));});
    child.stdin.on('error',()=>{});
    child.stderr.on('data',chunk=>{bytes+=chunk.length;if(bytes>limits.outputBytes)stop(new Error('output_limit'));});
    child.stdout.on('data',chunk=>{
      bytes+=chunk.length;
      if(bytes>limits.outputBytes){stop(new Error('output_limit'));return;}
      buffer+=decoder.write(chunk);
      let newline;
      while((newline=buffer.indexOf('\n'))>=0){
        const line=buffer.slice(0,newline);buffer=buffer.slice(newline+1);
        let message;try{message=JSON.parse(line);}catch{stop(new Error('sandbox_protocol'));return;}
        if(message.kind==='result'){stop(null,message.value);return;}
        if(message.kind!=='call'||!Number.isSafeInteger(message.id)||!allowed.has(message.name)||!message.args||typeof message.args!=='object'||Array.isArray(message.args)||Buffer.byteLength(JSON.stringify(message.args))>limits.argumentBytes){stop(new Error('capability_denied'));return;}
        if(++calls>limits.calls){stop(new Error('call_limit'));return;}
        if(busy){stop(new Error('concurrent_call_denied'));return;}
        busy=true;
        Promise.resolve().then(()=>callTool(message.name,message.args)).then(value=>{
          if(finished)return;
          const encoded=JSON.stringify({kind:'reply',id:message.id,value:{result:value}});
          if(Buffer.byteLength(encoded)>limits.outputBytes){stop(new Error('output_limit'));return;}
          child.stdin.write(encoded+'\n');
        }).catch(()=>{if(!finished)child.stdin.write(JSON.stringify({kind:'reply',id:message.id,value:{error:'tool_failed'}})+'\n');}).finally(()=>{busy=false;});
      }
    });
    child.stdin.write(JSON.stringify({kind:'execute',code})+'\n');
  });
}
