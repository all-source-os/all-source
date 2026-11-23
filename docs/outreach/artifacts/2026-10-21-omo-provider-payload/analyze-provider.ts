import {readFileSync,writeFileSync} from 'node:fs'
import {createHash} from 'node:crypto'
const root='/tmp/allsource-omo-compaction/provider',parse=(path:string)=>JSON.parse(readFileSync(path,'utf8')),sha=(value:string|Buffer)=>createHash('sha256').update(value).digest('hex')
function assert(value:unknown,message:string){if(!value)throw Error(message)}
function textOf(content:any):string{return typeof content==='string'?content:Array.isArray(content)?content.map(x=>typeof x==='string'?x:typeof x?.text==='string'?x.text:'').join('\n'):''}
const analysis=parse(`${root}/analysis.json`),arms:any[]=[],firstSettings=new Set<string>(),firstSchemas=new Set<string>()
const procedureFragments=['Call session_search with session_id=source_session, case_sensitive=true, limit=5 and EXACT checkpoint query from the map below.','Call session_info for that source_session.','Return the verified receipt from the matching search result, then stop.']
for(const arm of analysis.arms){
 const slot=arm.slot,fixture=parse(`${root}/${arm.pair}-shared-fixture.json`),hooks=readFileSync(`${root}/evidence/${slot}-hooks.jsonl`,'utf8').trim().split('\n').map(x=>JSON.parse(x)),requests=hooks.filter(x=>x.kind==='provider-request'),responses=hooks.filter(x=>x.kind==='provider-response')
 assert(requests.length>0&&requests.length===responses.length,'Missing provider responses')
 assert(!hooks.some(x=>x.kind==='provider-transport-error'),'Provider transport failure')
 const payloads=[]
 for(const [i,r] of requests.entries()){
  assert(r.sequence===i+1&&r.origin==='https://openrouter.ai'&&r.path==='/api/v1/chat/completions'&&r.requestUnchanged&&r.headersRetained===false,'Capture transport mismatch')
  assert(['checkpoint','compaction'].includes(r.phase)&&Number.isInteger(r.stage),'Unattributed model call')
  const bytes=readFileSync(`${root}/evidence/${r.bodyFile}`),body=JSON.parse(bytes.toString('utf8')),response=responses.find(x=>x.sequence===r.sequence)
  assert(sha(bytes)===r.bodySha256&&bytes.length===r.bodyBytes,'Captured body hash mismatch')
  assert(response?.status===200&&body.model==='minimax/minimax-m3.1-flash:free'&&body.stream===true,'Provider did not accept expected route')
  const summary=r.stage>0?arm.compactions.find((x:any)=>x.round===r.stage)?.summary:null
  const nativeTurn=parse(`${root}/evidence/${slot}-stage-${r.stage}.json`)
  const nativeSkillOutputs=nativeTurn.messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='tool'&&p.tool==='skill'&&p.state?.status==='completed').map((p:any)=>String(p.state.output))
  const messages=body.messages.map((m:any,index:number)=>{
   const text=textOf(m.content)
   const argumentsText=(m.tool_calls??[]).map((c:any)=>c.function?.arguments??'').join('\n')
   return {index,role:m.role,contentSha256:sha(text),textBytes:Buffer.byteLength(text),fullSkillBody:text.includes(fixture.skill.trim()),skillName:text.includes('zephyr-ledger-audit'),restorationHeader:text.includes('[restored skill instructions loaded earlier in this session; re-injected after compaction]'),recoveryPrefix:text.includes('[restore checkpointed session agent configuration after compaction]'),procedureFragments:procedureFragments.map((p,i)=>text.includes(p)?i:null).filter((x:any)=>x!==null),queryStages:fixture.fixtures.filter((f:any)=>text.includes(f.query)).map((f:any)=>f.stage),toolArgumentQueryStages:fixture.fixtures.filter((f:any)=>argumentsText.includes(f.query)).map((f:any)=>f.stage),receiptStages:fixture.fixtures.filter((f:any)=>text.includes(f.receipt)).map((f:any)=>f.stage),exactLatestSummary:!!summary&&text.includes(summary),exactInitialSkillToolOutput:nativeSkillOutputs.some((out:string)=>text.includes(out))}
  })
  const content=body.messages.map((m:any)=>textOf(m.content)).join('\n')
  payloads.push({sequence:r.sequence,at:r.at,phase:r.phase,stage:r.stage,file:r.bodyFile,bodySha256:r.bodySha256,bytes:r.bodyBytes,status:response.status,model:body.model,settings:{model:body.model,max_tokens:body.max_tokens,maxTokens:body.maxTokens,temperature:body.temperature,reasoning:body.reasoning,tool_choice:body.tool_choice,stream:body.stream},toolSchemaSha256:sha(JSON.stringify(body.tools??[])),toolNames:(body.tools??[]).map((t:any)=>t.function?.name),messages,fullSkillBody:messages.some((m:any)=>m.fullSkillBody),recoveryPrefix:messages.some((m:any)=>m.recoveryPrefix),restorationHeader:messages.some((m:any)=>m.restorationHeader),procedureFragments:procedureFragments.map((p,i)=>content.includes(p)?i:null).filter((x:any)=>x!==null),queryStages:fixture.fixtures.filter((f:any)=>content.includes(f.query)).map((f:any)=>f.stage),queryStagesAnywhere:fixture.fixtures.filter((f:any)=>bytes.includes(Buffer.from(f.query))).map((f:any)=>f.stage),currentQueryPresent:content.includes(fixture.fixtures[r.stage].query),currentQueryAnywhere:bytes.includes(Buffer.from(fixture.fixtures[r.stage].query)),currentReceiptPresent:content.includes(fixture.fixtures[r.stage].receipt),currentReceiptAnywhere:bytes.includes(Buffer.from(fixture.fixtures[r.stage].receipt))})
 }
 const checkpoints=[]
 for(let stage=0;stage<4;stage++){
  const turn=parse(`${root}/evidence/${slot}-stage-${stage}.json`),steps=turn.messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='step-start').length,group=payloads.filter(x=>x.phase==='checkpoint'&&x.stage===stage)
  assert(group.length===steps&&steps>0,'Provider checkpoint coverage does not match native steps')
  const first=group[0],task=turn.messages.find((m:any)=>m.info.role==='user'&&m.parts.some((p:any)=>p.text===turn.prompt))
  assert(first.at>=task.info.time.created,'First provider request preceded native task')
  assert(!first.currentReceiptAnywhere,'Receipt leaked before specialized source access')
  assert(JSON.stringify(first.toolNames)==='["session_info","session_search","skill"]','Unexpected first-call provider tools')
  firstSettings.add(JSON.stringify(first.settings));firstSchemas.add(first.toolSchemaSha256)
  if(stage===0){assert(!first.fullSkillBody&&!first.currentQueryAnywhere,'Baseline query leak');assert(group.some(x=>x.fullSkillBody&&x.messages.some((m:any)=>m.role==='tool'&&m.fullSkillBody&&m.exactInitialSkillToolOutput)),'Initial loaded skill absent from provider requests')}
  checkpoints.push({stage,nativeSteps:steps,providerRequests:group.length,firstSequence:first.sequence,firstFile:first.file,firstBodySha256:first.bodySha256,firstFullSkillBody:first.fullSkillBody,firstRecoveryPrefix:first.recoveryPrefix,firstRestorationHeader:first.restorationHeader,firstProcedureFragments:first.procedureFragments,firstCurrentQueryPresent:first.currentQueryPresent,firstCurrentQueryAnywhere:first.currentQueryAnywhere,firstQueryStages:first.queryStages,firstQueryStagesAnywhere:first.queryStagesAnywhere,firstCurrentReceiptPresent:first.currentReceiptPresent,firstToolSchemaSha256:first.toolSchemaSha256,firstSettings:first.settings,fullSkillBodyAnyCall:group.some(x=>x.fullSkillBody),currentQueryAnyCall:group.some(x=>x.currentQueryAnywhere),nativePass:arm.stages[stage].passed,sequences:group.map(x=>x.sequence)})
 }
 const compactions=[]
 for(let round=1;round<=3;round++){
  const data=parse(`${root}/evidence/${slot}-compaction-${round}.json`),summary=data.messages.filter((m:any)=>m.info.summary===true).at(-1),steps=summary.parts.filter((p:any)=>p.type==='step-start').length,group=payloads.filter(x=>x.phase==='compaction'&&x.stage===round)
  assert(group.length===steps&&steps>0,'Provider compaction coverage does not match native steps')
  const text=arm.compactions.find((c:any)=>c.round===round).summary
  compactions.push({round,nativeSteps:steps,sequences:group.map(x=>x.sequence),inputFullSkillBody:group.some(x=>x.fullSkillBody),inputQueryStages:group[0].queryStages,summaryFullSkillBody:text.includes(fixture.skill.trim()),summaryProcedureFragments:procedureFragments.map((p,i)=>text.includes(p)?i:null).filter((x:any)=>x!==null),summaryQueryStages:fixture.fixtures.filter((f:any)=>text.includes(f.query)).map((f:any)=>f.stage)})
 }
 assert(payloads.length===checkpoints.reduce((n,x)=>n+x.nativeSteps,0)+compactions.reduce((n,x)=>n+x.nativeSteps,0),'Unaccounted provider request')
 arms.push({slot,skillText:arm.skillText,checkpoints,compactions,requests:payloads.length,payloads})
}
assert(firstSettings.size===1&&firstSchemas.size===1,'First-call provider settings or schemas differ')
const result={date:'2026-10-21',transport:'Unchanged HTTPS fetch to OpenRouter; UTF-8 request bodies captured before native fetch; headers and credentials not retained',requestCount:arms.reduce((n,a)=>n+a.requests,0),allAccepted200:true,firstCallSettingsIdentical:true,firstCallToolSchemasIdentical:true,firstCallToolSchemaSha256:[...firstSchemas][0],coverage:'One captured request per native step-start across every checkpoint and compaction',arms,limits:['Provider-bound request content, not provider-internal prompt transformations or model attention','No response stream bodies captured; native model outputs retained separately','Two reused synthetic fixtures and one requested model route']}
writeFileSync(`${root}/provider-analysis.json`,JSON.stringify(result,null,2))
console.log(JSON.stringify({requests:result.requestCount,arms:arms.map(a=>({slot:a.slot,requests:a.requests,firstFullSkill:a.checkpoints.map(c=>c.firstFullSkillBody),firstCurrentQuery:a.checkpoints.map(c=>c.firstCurrentQueryPresent),passes:a.checkpoints.map(c=>c.nativePass)}))}))
