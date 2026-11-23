import {mkdirSync,readFileSync,writeFileSync,appendFileSync} from 'node:fs'
const root='/tmp/allsource-omo-compaction/full',run=process.env.QA_RUN || 'initial'
const home=`${root}/runs/${run}`,evidence=`${root}/evidence`
for(const dir of [home,evidence,`${home}/config/opencode`,`${home}/.omo`])mkdirSync(dir,{recursive:true})
const key=process.env.OPENROUTER_API_KEY || readFileSync('/Users/decebaldobrica/.hermes/.env','utf8').split('\n').find(l=>l.startsWith('OPENROUTER_API_KEY='))?.slice('OPENROUTER_API_KEY='.length).trim().replace(/^['"]|['"]$/g,'')
if(!key)throw Error('Missing authorized OpenRouter credential')
const sizes=JSON.parse(process.env.QA_SIZES || '[1024]')
writeFileSync(`${home}/fixture.json`,JSON.stringify({sizes}))
for(let i=0;i<sizes.length;i++){
 const dir=`${home}/.opencode/skills/qa-${i}`;mkdirSync(dir,{recursive:true})
 const start=`BEGIN_${i}\n`,end=`\nEND_${i}`,body=start+'x'.repeat(sizes[i]-start.length-end.length)+end
 writeFileSync(`${dir}/SKILL.md`,`---\nname: qa-${i}\ndescription: Synthetic compaction payload fixture\n---\n\n${body}\n`)
}
const omo={telemetry:false,auto_update:false,disabled_mcps:['websearch','context7','grep_app'],codegraph:{enabled:false,telemetry:false},agents:{sisyphus:{model:'openrouter/openai/gpt-4.1-mini'}},disabled_hooks:['auto-update-checker']}
writeFileSync(`${home}/.omo/omo.jsonc`,JSON.stringify({'[opencode]':omo}))
const config={plugin:['/tmp/allsource-omo-compaction/full-plugin.ts'],model:'openrouter/openai/gpt-4.1-mini',provider:{openrouter:{options:{apiKey:'{env:OPENROUTER_API_KEY}'},models:{'openai/gpt-4.1-mini':{limit:{context:1047576,output:256},options:{maxTokens:256}}}}},permission:'deny'}
writeFileSync(`${home}/config/opencode/opencode.json`,JSON.stringify(config))
const env={PATH:process.env.PATH!,HOME:home,OPENCODE_TEST_HOME:home,XDG_DATA_HOME:`${home}/data`,XDG_CONFIG_HOME:`${home}/config`,XDG_STATE_HOME:`${home}/state`,XDG_CACHE_HOME:`${home}/cache`,OPENCODE_DISABLE_MODELS_FETCH:'true',OPENCODE_DISABLE_AUTOUPDATE:'true',OPENROUTER_API_KEY:key,QA_EVIDENCE:`${evidence}/${run}-hooks.jsonl`}
const child=Bun.spawn(['/tmp/allsource-omo-compaction/runtime/opencode','--log-level','DEBUG','--print-logs','serve','--hostname','127.0.0.1','--port','43821'],{cwd:home,env,stdout:Bun.file(`${evidence}/${run}-server.log`),stderr:Bun.file(`${evidence}/${run}-server.err`)})
const requests:any[]=[],abort=new AbortController();let events:Promise<void>|undefined
async function request(path:string,body?:unknown){const response=await fetch('http://127.0.0.1:43821'+path,{method:body===undefined?'GET':'POST',body:body===undefined?undefined:JSON.stringify(body),headers:{'Content-Type':'application/json'},signal:AbortSignal.timeout(45000)});const text=await response.text();let value:any;try{value=JSON.parse(text)}catch{value=text}requests.push({at:Date.now(),path,status:response.status});if(!response.ok)throw Error(`${path}: ${response.status} ${JSON.stringify(value).slice(0,1500)}`);return value}
try{
 let ready=false;for(let i=0;i<100;i++){try{await request('/global/health');ready=true;break}catch{await Bun.sleep(100)}}if(!ready)throw Error('Server failed health')
 const effectiveConfig=await request('/config')
 writeFileSync(`${evidence}/${run}-effective-config.json`,JSON.stringify(effectiveConfig,(key,value)=>key==='apiKey'?'[REDACTED]':value,2))
 const hooks=readFileSync(`${evidence}/${run}-hooks.jsonl`,'utf8')
 if(!hooks.includes('full-plugin-initialized'))throw Error('Full plugin did not initialize; inspect hook log')
 const response=await fetch('http://127.0.0.1:43821/event',{signal:abort.signal})
 events=(async()=>{const reader=response.body!.getReader();for(;;){const {done,value}=await reader.read();if(done)break;appendFileSync(`${evidence}/${run}-events.txt`,value)}})().catch(error=>{if(!abort.signal.aborted)throw error})
 const agents=await request('/agent');writeFileSync(`${evidence}/${run}-agents.json`,JSON.stringify(agents,null,2))
 const session=await request('/session',{title:`AllSource full OMO #7183 ${run}`})
 await request(`/session/${session.id}/message`,{agent:'build',model:{providerID:'openrouter',modelID:'openai/gpt-4.1-mini'},noReply:true,parts:[{type:'text',text:'QA_LOAD: preserve fixture marker during compaction. This is a controlled memory evaluation.'}]})
 const loaded=readFileSync(`${evidence}/${run}-hooks.jsonl`,'utf8').split('\n').filter(x=>x.includes('full-skill-tool-result'))
 if(loaded.length!==sizes.length)throw Error(`Expected ${sizes.length} native skill results, got ${loaded.length}`)
 for(let round=1;round<=Number(process.env.QA_ROUNDS||1);round++){
  if(round===3){await Bun.sleep(31000);await Bun.sleep(31000)}
  await request(`/session/${session.id}/summarize`,{providerID:'openrouter',modelID:'openai/gpt-4.1-mini',auto:false})
  await Bun.sleep(1700)
  const messages=await request(`/session/${session.id}/message`)
  writeFileSync(`${evidence}/${run}-round-${round}.json`,JSON.stringify({at:Date.now(),round,sizes,sessionID:session.id,messages},null,2))
  if(messages.some((m:any)=>m.info.error))throw Error('Native session contains model error; inspect saved messages')
 }
 console.log(JSON.stringify({run,sessionID:session.id,sizes,fullPlugin:true}))
}finally{
 abort.abort();await events
 child.kill('SIGINT');await Promise.race([child.exited,Bun.sleep(2000)]);if(child.exitCode===null)child.kill('SIGKILL');await child.exited
 writeFileSync(`${evidence}/${run}-requests.json`,JSON.stringify(requests,null,2))
}
