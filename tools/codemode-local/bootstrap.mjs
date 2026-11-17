// Reproducible optional dependencies live outside scanned source directories.
import { mkdir, copyFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';
const directory=fileURLToPath(new URL('.',import.meta.url));
const cache=resolve(process.env.FELIX_CODEMODE_CACHE??resolve(directory,'../../target/codemode-local'));
await mkdir(cache,{recursive:true});
for(const file of ['package.json','package-lock.json'])await copyFile(join(directory,file),join(cache,file));
const installed=spawnSync('npm',['ci','--prefix',cache,'--ignore-scripts','--no-audit','--no-fund'],{stdio:'inherit'});
if(installed.status!==0)process.exit(installed.status??1);
const esbuild=await import(pathToFileURL(join(cache,'node_modules/esbuild/lib/main.js')).href);
await esbuild.build({entryPoints:[join(directory,'worker.mjs')],bundle:true,format:'esm',platform:'neutral',external:['cloudflare:*'],nodePaths:[join(cache,'node_modules')],outfile:join(directory,'worker.bundle.mjs')});
