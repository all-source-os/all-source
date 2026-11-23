import {mkdirSync,readFileSync,writeFileSync,appendFileSync} from 'node:fs'
import {randomBytes} from 'node:crypto'
const root='/tmp/allsource-omo-compaction/model',run=process.env.QA_RUN || 'selected-v1'
const home=`${root}/runs/${run}`,evidence=`${root}/evidence`
for(const dir of [home,evidence,`${home}/config/opencode`,`${home}/.omo`,`${home}/.opencode/skills/zephyr-ledger-audit`,`${home}/.opencode/skills/canvas-layout`])mkdirSync(dir,{recursive:true})
const token=()=>randomBytes(5).toString('hex')
const fixtures=Array.from({length:4},(_,stage)=>({stage,query:`ZEPHYR_${token().toUpperCase()}`,receipt:`receipt-${stage}-${token()}`}))
const skill=`# Zephyr ledger audit\n\nFor a requested checkpoint N and source_session provided by the user, execute this procedure.\n1. Call session_search with session_id=source_session, case_sensitive=true, limit=5, and EXACT query from the checkpoint map below.\n2. Call session_info for the same source_session to validate the source exists.\n3. Return ONLY the literal receipt from that search result after the source check succeeds. Then stop. Never search for the receipt itself or substitute a different checkpoint query. Do not invent a receipt or use general file tools. Repeat this procedure for each new checkpoint.\n\nCheckpoint query map:\n${fixtures.map(x=>`${x.stage}: ${x.query}`).join('\n')}\n\nIf these instructions remain in your context, use them directly without loading this skill again.\nEND_ZEPHYR_SKILL\n`
writeFileSync(`${home}/.opencode/skills/zephyr-ledger-audit/SKILL.md`,`---\nname: zephyr-ledger-audit\ndescription: Verify a Zephyr ledger checkpoint using specialized session search and source inspection tools\n---\n\n${skill}`)
writeFileSync(`${home}/.opencode/skills/canvas-layout/SKILL.md`,'---\nname: canvas-layout\ndescription: Arrange chart labels and screen layouts\n---\n\nUse this skill for visual layout work only. END_DECOY\n')
writeFileSync(`${evidence}/${run}-fixtures.json`,JSON.stringify({skill,fixtures},null,2))
const key=process.env.OPENROUTER_API_KEY || readFileSync('/Users/decebaldobrica/.hermes/.env','utf8').split('\n').find(l=>l.startsWith('OPENROUTER_API_KEY='))?.slice(19).trim().replace(/^['"]|['"]$/g,'')
if(!key)throw Error('Missing authorized OpenRouter credential')
const model=process.env.QA_MODEL || 'openai/gpt-4.1-mini'
const omo={telemetry:false,auto_update:false,disabled_mcps:['websearch','context7','grep_app','lsp'],codegraph:{enabled:false,telemetry:false},agents:{sisyphus:{model:`openrouter/${model}`}},disabled_hooks:['auto-update-checker']}
writeFileSync(`${home}/.omo/omo.jsonc`,JSON.stringify({'[opencode]':omo}))
const permission={'*':'deny',skill:'allow',session_search:'allow',session_info:'allow'}
const shortPrompt='You are a controlled ledger-audit assistant. Select the task-matching installed skill through available tools before acting. Follow its exact procedure. Use instructions already in context instead of reloading them. Never guess identifiers. Once the required tool evidence is available, return the receipt verbatim and stop. Do not narrate future actions.'
const maxOutput=model.endsWith(':free')?2048:512
const config={plugin:['/tmp/allsource-omo-compaction/model-observer.ts'],model:`openrouter/${model}`,agent:{build:{prompt:shortPrompt,maxSteps:5}},mcp:{lsp:{enabled:false}},provider:{openrouter:{options:{apiKey:'{env:OPENROUTER_API_KEY}'},models:{[model]:{limit:{context:1047576,output:maxOutput},options:{maxTokens:maxOutput,temperature:0,reasoning:{effort:'low'}}}}}},permission}
writeFileSync(`${home}/config/opencode/opencode.json`,JSON.stringify(config))
const env={PATH:process.env.PATH!,HOME:home,OPENCODE_TEST_HOME:home,XDG_DATA_HOME:`${home}/data`,XDG_CONFIG_HOME:`${home}/config`,XDG_STATE_HOME:`${home}/state`,XDG_CACHE_HOME:`${home}/cache`,OPENCODE_DISABLE_MODELS_FETCH:'true',OPENCODE_DISABLE_AUTOUPDATE:'true',OPENROUTER_API_KEY:key,QA_EVIDENCE:`${evidence}/${run}-hooks.jsonl`}
const child=Bun.spawn(['/tmp/allsource-omo-compaction/runtime/opencode','--log-level','DEBUG','--print-logs','serve','--hostname','127.0.0.1','--port','43822'],{cwd:home,env,stdout:Bun.file(`${evidence}/${run}-server.log`),stderr:Bun.file(`${evidence}/${run}-server.err`)})
const requests:any[]=[],abort=new AbortController();let events:Promise<void>|undefined
async function request(path:string,body?:unknown){const response=await fetch('http://127.0.0.1:43822'+path,{method:body===undefined?'GET':'POST',body:body===undefined?undefined:JSON.stringify(body),headers:{'Content-Type':'application/json'},signal:AbortSignal.timeout(55000)});const text=await response.text();let value:any;try{value=JSON.parse(text)}catch{value=text}requests.push({at:Date.now(),path,status:response.status,body});if(!response.ok)throw Error(`${path}: ${response.status} ${JSON.stringify(value).slice(0,1000)}`);return value}
const tools={skill:true,session_search:true,session_info:true,session_read:false,session_list:false,bash:false,read:false,write:false,edit:false,glob:false,grep:false,task:false,call_omo_agent:false,look_at:false,interactive_bash:false,skill_mcp:false}
let failure:unknown
try{
 let ready=false;for(let i=0;i<100;i++){try{await request('/global/health');ready=true;break}catch{await Bun.sleep(100)}}if(!ready)throw Error('Server failed health')
 const effective=await request('/config');writeFileSync(`${evidence}/${run}-effective-config.json`,JSON.stringify(effective,(key,value)=>key==='apiKey'?'[REDACTED]':value,2))
 if(Object.values(effective.mcp || {}).some((x:any)=>x.enabled!==false))throw Error('Unexpected enabled MCP integration')
 const response=await fetch('http://127.0.0.1:43822/event',{signal:abort.signal})
 events=(async()=>{const reader=response.body!.getReader();for(;;){const {done,value}=await reader.read();if(done)break;appendFileSync(`${evidence}/${run}-events.txt`,value)}})().catch(error=>{if(!abort.signal.aborted)throw error})
 const sources=[]
 for(const fixture of fixtures){const source=await request('/session',{title:`Zephyr source checkpoint ${fixture.stage}`});sources.push(source.id);await request(`/session/${source.id}/message`,{agent:'build',model:{providerID:'openrouter',modelID:model},noReply:true,parts:[{type:'text',text:`${fixture.query} ${fixture.receipt}. Synthetic ledger record.`}]})}
 const session=await request('/session',{title:`AllSource model-selected OMO #7183 ${run}`})
 writeFileSync(`${evidence}/${run}-identity.json`,JSON.stringify({sessionID:session.id,sources,model},null,2))
 for(let stage=0;stage<4;stage++){
  if(stage>0){if(stage===3){await Bun.sleep(31000);await Bun.sleep(31000)}await request(`/session/${session.id}/summarize`,{providerID:'openrouter',modelID:model,auto:false});await Bun.sleep(1200);const messages=await request(`/session/${session.id}/message`);writeFileSync(`${evidence}/${run}-compaction-${stage}.json`,JSON.stringify({at:Date.now(),stage,messages},null,2))}
  const metadata=stage===0?'Available installed skill metadata: zephyr-ledger-audit — verify Zephyr ledger checkpoints; canvas-layout — arrange chart labels and screen layouts. Select the appropriate skill and follow its procedure. Do not guess opaque ledger queries. ':''
  const prompt=`${metadata}Run Zephyr ledger audit checkpoint ${stage}. source_session=${sources[stage]}. Return the verified receipt.`
  const before=await request(`/session/${session.id}/message`),known=new Set(before.map((m:any)=>m.info.id))
  const result=await request(`/session/${session.id}/message`,{agent:'build',model:{providerID:'openrouter',modelID:model},tools,parts:[{type:'text',text:prompt}]})
  const messages=await request(`/session/${session.id}/message`)
  const delta=messages.filter((m:any)=>!known.has(m.info.id))
  writeFileSync(`${evidence}/${run}-stage-${stage}.json`,JSON.stringify({at:Date.now(),stage,prompt,expected:fixtures[stage],sourceSessionID:sources[stage],messages:delta},null,2))
  if(delta.some((m:any)=>m.info.error))throw Error(`Stage ${stage}: terminal model error`)
  const calls=delta.flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='tool')
  const search=calls.find((p:any)=>p.tool==='session_search'&&p.state?.status==='completed'&&p.state.input?.query===fixtures[stage].query&&p.state.input?.session_id===sources[stage]&&String(p.state.output).includes(fixtures[stage].receipt))
  const info=calls.find((p:any)=>p.tool==='session_info'&&p.state?.status==='completed'&&p.state.input?.session_id===sources[stage]&&String(p.state.output).includes(sources[stage]))
  if(!search||!info)throw Error(`Stage ${stage}: missing successful exact-query search or source-info tool execution`)
  const finalText=delta.filter((m:any)=>m.info.role==='assistant').flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='text').map((p:any)=>p.text).join('\n')
  if(!finalText.includes(fixtures[stage].receipt))throw Error(`Stage ${stage}: expected literal receipt missing from model answer`)
  if(stage===0&&!calls.some((p:any)=>p.tool==='skill'&&p.state?.status==='completed'&&p.state.input?.name==='zephyr-ledger-audit'))throw Error('No model-selected native skill call')
 }
 console.log(JSON.stringify({run,model,stagesPassed:4}))
}catch(error){failure=String(error);writeFileSync(`${evidence}/${run}-failure.json`,JSON.stringify({at:Date.now(),error:failure},null,2));throw error}finally{
 abort.abort();await events
 child.kill('SIGINT');await Promise.race([child.exited,Bun.sleep(2000)]);if(child.exitCode===null)child.kill('SIGKILL');await child.exited
 writeFileSync(`${evidence}/${run}-requests.json`,JSON.stringify(requests,null,2))
}
