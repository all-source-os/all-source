import {mkdirSync,readFileSync,writeFileSync,appendFileSync,existsSync,cpSync} from 'node:fs'
import {randomBytes,createHash} from 'node:crypto'
const root='/tmp/allsource-omo-compaction/skill-only',slot=process.env.QA_SLOT!,recovery=process.env.QA_SKILL_TEXT!,pair=process.env.QA_PAIR!
if(!pair||!slot||!['on','off'].includes(recovery))throw Error('Set QA_SLOT and QA_RECOVERY=on/off')
const home=`${root}/runs/${slot}`,evidence=`${root}/evidence`,policyFile=`${home}/policy.json`
if(existsSync(home))throw Error('Use a fresh slot; do not mix native profiles')
for(const dir of [home,evidence,`${home}/config/opencode`,`${home}/.omo`,`${home}/.opencode/skills/zephyr-ledger-audit`,`${home}/.opencode/skills/canvas-layout`])mkdirSync(dir,{recursive:true})
const shared=`${root}/${pair}-shared-fixture.json`
if(!existsSync(shared)){
 const token=()=>randomBytes(5).toString('hex')
 const fixtures=Array.from({length:4},(_,stage)=>({stage,query:`ZEPHYR_${token().toUpperCase()}`,receipt:`receipt-${stage}-${token()}`}))
 const skill=`# Zephyr ledger audit\n\nFor checkpoint N and source_session supplied by the user:\n1. Call session_search with session_id=source_session, case_sensitive=true, limit=5 and EXACT checkpoint query from the map below.\n2. Call session_info for that source_session.\n3. Return the verified receipt from the matching search result, then stop. Never guess a query or invent a receipt. General file access, broad searches and searching other sessions are unavailable. If instructions remain in context, use them without reloading.\n\nCheckpoint query map:\n${fixtures.map(x=>`${x.stage}: ${x.query}`).join('\n')}\nEND_ZEPHYR_SKILL\n`
 writeFileSync(shared,JSON.stringify({fixtures,skill},null,2))
}
const fixture=JSON.parse(readFileSync(shared,'utf8'))
writeFileSync(`${home}/.opencode/skills/zephyr-ledger-audit/SKILL.md`,`---\nname: zephyr-ledger-audit\ndescription: Verify Zephyr ledger checkpoints with specialized session search and source inspection\n---\n\n${fixture.skill}`)
writeFileSync(`${home}/.opencode/skills/canvas-layout/SKILL.md`,'---\nname: canvas-layout\ndescription: Arrange chart labels and screen layouts\n---\n\nVisual layouts only. END_DECOY\n')
writeFileSync(policyFile,JSON.stringify({subject:null,source:null}))
const key=process.env.OPENROUTER_API_KEY||readFileSync('/Users/decebaldobrica/.hermes/.env','utf8').split('\n').find(x=>x.startsWith('OPENROUTER_API_KEY='))?.slice(19).trim().replace(/^['"]|['"]$/g,'')
if(!key)throw Error('Missing authorized OpenRouter credential')
const model='minimax/minimax-m3.1-flash:free'
const omo={telemetry:false,auto_update:false,disabled_mcps:['websearch','context7','grep_app','lsp'],codegraph:{enabled:false,telemetry:false},agents:{sisyphus:{model:`openrouter/${model}`}},disabled_hooks:['auto-update-checker']}
writeFileSync(`${home}/.omo/omo.jsonc`,JSON.stringify({'[opencode]':omo}))
const prompt='You are a controlled ledger-audit assistant. Select the matching installed skill through available tools. Follow its procedure exactly. Use instructions already in context instead of reloading. Never guess identifiers. Return the verified receipt and stop once required tool evidence is available. If evidence cannot be obtained, state that plainly. Do not narrate future actions.'
const permission={'*':'deny',skill:'allow',session_search:'allow',session_info:'allow'}
const config={plugin:['/tmp/allsource-omo-compaction/skill-only-observer.ts'],model:`openrouter/${model}`,agent:{build:{prompt,maxSteps:5}},mcp:{lsp:{enabled:false}},provider:{openrouter:{options:{apiKey:'{env:OPENROUTER_API_KEY}'},models:{[model]:{limit:{context:1000000,output:2048},options:{maxTokens:2048,temperature:0,reasoning:{effort:'low'}}}}}},permission}
writeFileSync(`${home}/config/opencode/opencode.json`,JSON.stringify(config))
writeFileSync(`${evidence}/${slot}-manifest.json`,JSON.stringify({slot,pair,skillText:recovery,model,fixtureSha256:createHash('sha256').update(readFileSync(shared)).digest('hex'),config,omo,policy:'initial native skill succeeds once; all subsequent skill executions throw before native code; source-scoped opaque searches only'},null,2))
const env={PATH:process.env.PATH!,HOME:home,OPENCODE_TEST_HOME:home,XDG_DATA_HOME:`${home}/data`,XDG_CONFIG_HOME:`${home}/config`,XDG_STATE_HOME:`${home}/state`,XDG_CACHE_HOME:`${home}/cache`,OPENCODE_DISABLE_MODELS_FETCH:'true',OPENCODE_DISABLE_AUTOUPDATE:'true',OPENROUTER_API_KEY:key,QA_EVIDENCE:`${evidence}/${slot}-hooks.jsonl`,QA_POLICY:policyFile,QA_SKILL_TEXT:recovery}
for(const name of ['node_modules','package.json','package-lock.json'])cpSync('/tmp/allsource-omo-compaction/matched/runs/b1/config/opencode/'+name,home+'/config/opencode/'+name,{recursive:true})
for(const name of ['node_modules','package.json','package-lock.json'])cpSync('/tmp/allsource-omo-compaction/matched/runs/b1/.opencode/'+name,home+'/.opencode/'+name,{recursive:true})
const child=Bun.spawn(['/tmp/allsource-omo-compaction/runtime/opencode','--log-level','DEBUG','--print-logs','serve','--hostname','127.0.0.1','--port','43823'],{cwd:home,env,stdout:Bun.file(`${evidence}/${slot}-server.log`),stderr:Bun.file(`${evidence}/${slot}-server.err`)})
const requests:any[]=[],abort=new AbortController();let events:Promise<void>|undefined
async function request(path:string,body?:unknown){const response=await fetch('http://127.0.0.1:43823'+path,{method:body===undefined?'GET':'POST',body:body===undefined?undefined:JSON.stringify(body),headers:{'Content-Type':'application/json'},signal:AbortSignal.timeout(50000)});const text=await response.text();let value:any;try{value=JSON.parse(text)}catch{value=text}requests.push({at:Date.now(),path,status:response.status,body});if(!response.ok)throw Error(`${path}: HTTP ${response.status}`);return value}
const tools={skill:true,session_search:true,session_info:true,session_read:false,session_list:false,bash:false,read:false,write:false,edit:false,glob:false,grep:false,task:false,call_omo_agent:false,look_at:false,interactive_bash:false,skill_mcp:false}
const scores:any[]=[]
try{
 let ready=false;for(let i=0;i<100;i++){try{await request('/global/health');ready=true;break}catch{await Bun.sleep(100)}}if(!ready)throw Error('Server failed health')
 const effective=await request('/config');writeFileSync(`${evidence}/${slot}-effective-config.json`,JSON.stringify(effective,(key,value)=>key==='apiKey'?'[REDACTED]':value,2));if(Object.values(effective.mcp||{}).some((x:any)=>x.enabled!==false))throw Error('Unexpected enabled MCP')
 const response=await fetch('http://127.0.0.1:43823/event',{signal:abort.signal})
 events=(async()=>{const reader=response.body!.getReader();for(;;){const {done,value}=await reader.read();if(done)break;appendFileSync(`${evidence}/${slot}-events.txt`,value)}})().catch(error=>{if(!abort.signal.aborted)throw error})
 const sources=[]
 for(const row of fixture.fixtures){const source=await request('/session',{title:`Zephyr source checkpoint ${row.stage}`});sources.push(source.id);await request(`/session/${source.id}/message`,{agent:'build',model:{providerID:'openrouter',modelID:model},noReply:true,parts:[{type:'text',text:`${row.query} ${row.receipt}. Synthetic ledger record.`}]})}
 const subject=await request('/session',{title:'Matched ledger audit'})
 writeFileSync(`${evidence}/${slot}-identity.json`,JSON.stringify({subject:subject.id,sources},null,2))
 for(let stage=0;stage<4;stage++){
  writeFileSync(policyFile,JSON.stringify({subject:subject.id,source:sources[stage]}))
  if(stage>0){if(stage===3){await Bun.sleep(31000);await Bun.sleep(31000)}await request(`/session/${subject.id}/summarize`,{providerID:'openrouter',modelID:model,auto:false});await Bun.sleep(1400);const messages=await request(`/session/${subject.id}/message`);writeFileSync(`${evidence}/${slot}-compaction-${stage}.json`,JSON.stringify({at:Date.now(),stage,messages},null,2))}
  const metadata=stage===0?'Available installed skills: zephyr-ledger-audit — verify ledger checkpoints; canvas-layout — arrange chart labels. Select the appropriate skill. ':''
  const text=`${metadata}Run Zephyr ledger audit checkpoint ${stage}. source_session=${sources[stage]}. Return the verified receipt.`
  const before=await request(`/session/${subject.id}/message`),known=new Set(before.map((m:any)=>m.info.id))
  await request(`/session/${subject.id}/message`,{agent:'build',model:{providerID:'openrouter',modelID:model},tools,parts:[{type:'text',text}]})
  const messages=(await request(`/session/${subject.id}/message`)).filter((m:any)=>!known.has(m.info.id))
  const calls=messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='tool'),expected=fixture.fixtures[stage]
  const search=calls.some((p:any)=>p.tool==='session_search'&&p.state?.status==='completed'&&p.state.input?.query===expected.query&&p.state.input?.session_id===sources[stage]&&String(p.state.output).includes(expected.receipt))
  const info=calls.some((p:any)=>p.tool==='session_info'&&p.state?.status==='completed'&&p.state.input?.session_id===sources[stage]&&String(p.state.output).startsWith(`Session ID: ${sources[stage]}`))
  const answer=messages.filter((m:any)=>m.info.role==='assistant').flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='text').map((p:any)=>p.text).join('\n')
  const errors=messages.filter((m:any)=>m.info.error).map((m:any)=>m.info.error)
  const score={stage,search,info,receipt:answer.includes(expected.receipt),passed:search&&info&&answer.includes(expected.receipt)&&errors.length===0,errors:errors.map((e:any)=>({name:e.name,message:e.data?.message})),skillCalls:calls.filter((p:any)=>p.tool==='skill').map((p:any)=>({status:p.state?.status,input:p.state?.input,error:p.state?.error}))}
  scores.push(score);writeFileSync(`${evidence}/${slot}-stage-${stage}.json`,JSON.stringify({at:Date.now(),stage,prompt:text,expected,sourceSessionID:sources[stage],score,messages},null,2))
  if(errors.length)throw Error(`Stage ${stage}: provider/runtime interruption`)
  if(stage===0&&!score.passed)throw Error('Initial condition failed; do not interpret recovery contrast')
 }
 console.log(JSON.stringify({slot,recovery,scores}))
}catch(error){writeFileSync(`${evidence}/${slot}-failure.json`,JSON.stringify({at:Date.now(),error:String(error)},null,2));throw error}finally{
 abort.abort();await events
 child.kill('SIGINT');await Promise.race([child.exited,Bun.sleep(2000)]);if(child.exitCode===null)child.kill('SIGKILL');await child.exited
 writeFileSync(`${evidence}/${slot}-requests.json`,JSON.stringify(requests,null,2));writeFileSync(`${evidence}/${slot}-scores.json`,JSON.stringify(scores,null,2))
}
