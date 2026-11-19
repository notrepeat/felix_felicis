import { spawn } from 'node:child_process';
import { StringDecoder } from 'node:string_decoder';
export class McpClient {
  constructor(binary, config) {
    this.child=spawn(binary,['--root',config.root,'--manifiesto',config.manifest,'--invariantes',config.invariants],{stdio:['pipe','pipe','pipe'],detached:process.platform==='linux'});
    this.closed=new Promise(resolve=>this.child.once('close',resolve));
    this.pending=new Map();this.id=0;this.bytesIn=0;this.bytesOut=0;this.calls=0;
    let buffer='';const decoder=new StringDecoder('utf8');
    this.child.stdout.on('data',chunk=>{
      this.bytesOut+=chunk.length;buffer+=decoder.write(chunk);
      if(Buffer.byteLength(buffer)>2*1024*1024){this.kill();return;}
      let end;while((end=buffer.indexOf('\n'))>=0){
        const line=buffer.slice(0,end);buffer=buffer.slice(end+1);
        let value;try{value=JSON.parse(line);}catch{this.kill();return;}
        const pending=this.pending.get(value.id);if(pending){this.pending.delete(value.id);clearTimeout(pending.timer);value.error?pending.reject(new Error('mcp_protocol')):pending.resolve(value.result);}
      }
    });
    this.child.stderr.resume();
    this.child.stdin.on('error',()=>{});
    this.child.on('error',()=>this.fail());this.child.on('close',()=>this.fail());
  }
  kill(){try{if(process.platform==='linux')process.kill(-this.child.pid,'SIGKILL');else this.child.kill();}catch{}}
  fail(){for(const pending of this.pending.values()){clearTimeout(pending.timer);pending.reject(new Error('mcp_closed'));}this.pending.clear();}
  send(value){const line=JSON.stringify(value)+'\n';this.bytesIn+=Buffer.byteLength(line);this.child.stdin.write(line);}
  request(method,params){return new Promise((resolve,reject)=>{
    const id=++this.id;
    const timer=setTimeout(()=>{this.pending.delete(id);this.kill();reject(new Error('mcp_timeout'));},10_000);
    this.pending.set(id,{resolve,reject,timer});this.send({jsonrpc:'2.0',id,method,params});
  });}
  async initialize(){await this.request('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'local-codemode-comparison',version:'1'}});this.send({jsonrpc:'2.0',method:'notifications/initialized'});}
  async tool(name,args){this.calls++;const result=await this.request('tools/call',{name,arguments:args});if(result.isError)throw new Error('mcp_tool_error');return result.structuredContent;}
  async close(){this.kill();await this.closed;}
}
