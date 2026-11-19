import { DynamicWorkerExecutor } from '@cloudflare/codemode';
export default {
  async fetch(request, env) {
    const { code } = await request.json();
    const fns = Object.fromEntries(['context', 'propose', 'check', 'why'].map(name => [name, async args => {
      const response = await env.BRIDGE.fetch('http://bridge/tool', { method: 'POST', body: JSON.stringify({name,args}) });
      const value = await response.json();
      if (value.error) throw new Error(value.error);
      return value.result;
    }]));
    const executor = new DynamicWorkerExecutor({loader:env.LOADER, timeout:10_000, globalOutbound:null});
    const result = await executor.execute(code, [{name:'felix',fns}]);
    return Response.json(result);
  }
};
