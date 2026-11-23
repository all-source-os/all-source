import {readFileSync,writeFileSync} from 'node:fs'
import {createHash} from 'node:crypto'
const root='/tmp/allsource-omo-compaction',work=`${root}/skill-only`,repo='/Users/decebaldobrica/Projects/founder-mode/all-source'
const parse=(path:string)=>JSON.parse(readFileSync(path,'utf8')),sha=(x:string|Buffer)=>createHash('sha256').update(x).digest('hex')
function assert(ok:unknown,message:string){if(!ok)throw Error(message)}
const slots=['s1a','s1b','s2a','s2b'],conditions=['on','off','off','on'],arms:any[]=[],contexts=new Set<string>()
let hostConfig:string|undefined,omoConfig:string|undefined,envelope:string|undefined,dependencyLocks:string|undefined
for(let i=0;i<slots.length;i++){
 const slot=slots[i],manifest=parse(`${work}/evidence/${slot}-manifest.json`),fixture=parse(`${work}/${manifest.pair}-shared-fixture.json`)
 assert(manifest.skillText===conditions[i],'Order mismatch')
 assert(manifest.fixtureSha256===sha(readFileSync(`${work}/${manifest.pair}-shared-fixture.json`)),'Fixture mismatch')
 assert(!manifest.omo.disabled_hooks.includes('compaction-context-injector'),'Native recovery disabled')
 const config=JSON.stringify(manifest.config),omo=JSON.stringify(manifest.omo)
 assert(!hostConfig||hostConfig===config,'Host config differs');hostConfig=config
 assert(!omoConfig||omoConfig===omo,'OMO config differs');omoConfig=omo
 const locks=JSON.stringify(['config/opencode/package-lock.json','.opencode/package-lock.json'].map(path=>sha(readFileSync(`${work}/runs/${slot}/${path}`))))
 assert(!dependencyLocks||dependencyLocks===locks,'Native dependency locks differ');dependencyLocks=locks
 const identity=parse(`${work}/evidence/${slot}-identity.json`)
 const hooks=readFileSync(`${work}/evidence/${slot}-hooks.jsonl`,'utf8').trim().split('\n').map(x=>JSON.parse(x))
 const init=hooks.filter(x=>x.kind==='full-plugin-initialized');assert(init.length===1&&init[0].tools.length===14&&init[0].hooks.length===15,'Full native plugin missing')
 const loads=hooks.filter(x=>x.kind==='native-tool-completed'&&x.tool==='skill'),blocks=hooks.filter(x=>x.kind==='reload-blocked')
 assert(loads.length===1&&loads[0].args.name==='zephyr-ledger-audit'&&hooks.filter(x=>x.kind==='reload-gate-closed').length===1,'Initial load latch failed')
 assert(blocks.every(x=>x.nativeInvoked===false),'Reload bypass')
 const dispatches=hooks.filter(x=>x.kind==='recovery-dispatch-control'),responses=hooks.filter(x=>x.kind==='recovery-dispatch-response')
 assert(dispatches.length===2&&responses.length===2&&responses.every(x=>x.status>=200&&x.status<300),'Dispatch count or acceptance mismatch')
 for(const d of dispatches){
  assert(d.nonSkillFieldsIdentical&&d.changed===(conditions[i]==='off'?1:0),'Wrong dispatch intervention')
  assert(d.before.agent&&d.before.model&&d.before.tools&&d.before.noReply===true,'Agent/model/tools/noReply missing')
  const {parts,...before}=d.before,{parts:afterParts,...after}=d.after
  assert(JSON.stringify(before)===JSON.stringify(after),'Envelope changed')
  const shape=JSON.stringify(before);assert(!envelope||envelope===shape,'Recovery envelope differs across arms');envelope=shape
  assert(parts.length===1&&afterParts.length===1,'Unexpected parts')
  const {text,...part}=parts[0],{text:afterText,...afterPart}=afterParts[0]
  assert(JSON.stringify(part)===JSON.stringify(afterPart),'Part metadata changed')
  const prefix='[restore checkpointed session agent configuration after compaction]',suffix='\n<!-- OMO_INTERNAL_INITIATOR -->\n<!-- OMO_INTERNAL_NOREPLY -->'
  assert(text.includes(fixture.skill.trim())&&text.endsWith(suffix),'Original recovery incomplete')
  assert(afterText===(conditions[i]==='off'?prefix+suffix:text),'Unexpected text edit')
 }
 const injected=hooks.filter(x=>x.kind==='summary-injection-control')
 assert(injected.length===3,'Summary hook count mismatch')
 for(const h of injected){assert(h.output.context.length===1&&h.output.context[0].startsWith('[SYSTEM DIRECTIVE: OH-MY-OPENCODE - COMPACTION CONTEXT]'),'Native summary injection missing');contexts.add(JSON.stringify(h.output))}
 const events=hooks.filter(x=>x.kind==='hook-enter'&&x.name==='event'&&x.eventType==='session.compacted')
 assert(events.length===3,'Compaction count mismatch')
 const gap12=events[1].at-events[0].at,gap13=events[2].at-events[0].at
 assert(gap12<60000&&gap13>60000,'Cooldown regimes differ')
 assert(dispatches[0].at>=events[0].at&&dispatches[0].at<events[1].at&&dispatches[1].at>=events[2].at,'Cooldown dispatch timing mismatch')
 const stages=[]
 for(let stage=0;stage<4;stage++){
  const d=parse(`${work}/evidence/${slot}-stage-${stage}.json`),calls=d.messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='tool'),row=fixture.fixtures[stage],source=identity.sources[stage]
  assert(!d.messages.some((m:any)=>m.info.error),'Provider/runtime interruption')
  assert(!d.prompt.includes(row.query)&&!d.prompt.includes(row.receipt),'Task leaks oracle')
  const search=calls.some((p:any)=>p.tool==='session_search'&&p.state.status==='completed'&&p.state.input.query===row.query&&p.state.input.session_id===source&&String(p.state.output).includes(row.receipt))
  const info=calls.some((p:any)=>p.tool==='session_info'&&p.state.status==='completed'&&p.state.input.session_id===source&&String(p.state.output).startsWith(`Session ID: ${source}`))
  const answer=d.messages.filter((m:any)=>m.info.role==='assistant').flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='text').map((p:any)=>p.text).join('\n'),receipt=answer.includes(row.receipt),passed=search&&info&&receipt
  assert(passed===d.score.passed,'Score mismatch')
  if(stage===0)assert(passed,'Baseline failed')
  else assert(!calls.some((p:any)=>p.tool==='skill'&&p.state.status==='completed'),'Reload succeeded')
  stages.push({stage,passed,search,info,receipt,answer,calls:calls.map((p:any)=>({tool:p.tool,status:p.state.status,input:p.state.input,error:p.state.error}))})
 }
 const compactions=[]
 for(let round=1;round<=3;round++){
  const d=parse(`${work}/evidence/${slot}-compaction-${round}.json`),summaries=d.messages.filter((m:any)=>m.info.summary===true)
  assert(summaries.length===round&&!d.messages.some((m:any)=>m.info.error),'Native summary failed')
  const rawRecovery=d.messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.synthetic&&p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))
  const turn=parse(`${work}/evidence/${slot}-stage-${round}.json`),byID=new Map([...d.messages,...turn.messages].map((m:any)=>[m.info.id,m]))
  const recovery=[...byID.values()].flatMap((m:any)=>m.parts).filter((p:any)=>p.synthetic&&p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))
  assert(recovery.length===[1,1,2][round-1],'Stored config recovery count mismatch')
  assert(recovery.every((p:any)=>p.text.includes('END_ZEPHYR_SKILL')===(conditions[i]==='on')),'Stored skill treatment mismatch')
  const late=dispatches.filter(x=>x.at>d.at&&x.at<turn.at)
  const taskMessage=turn.messages.find((m:any)=>m.info.role==='user'&&m.parts.some((p:any)=>p.type==='text'&&p.text===turn.prompt))
  const duringTurn=turn.messages.filter((m:any)=>m.parts.some((p:any)=>p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))).map((m:any)=>({createdAt:m.info.time.created,afterTaskMessageMs:m.info.time.created-taskMessage.info.time.created}))
  compactions.push({round,rawSnapshotConfigRecoveries:rawRecovery.length,configRecoveries:recovery.length,skillRecoveries:recovery.filter((p:any)=>p.text.includes('END_ZEPHYR_SKILL')).length,lateDispatches:late.map(x=>({at:x.at,afterSnapshotMs:x.at-d.at})),recoveryMessagesDuringTurn:duringTurn,summary:summaries.at(-1).parts.filter((p:any)=>p.type==='text').map((p:any)=>p.text).join('\n')})
 }
 arms.push({slot,pair:manifest.pair,skillText:manifest.skillText,stages,compactions,blockedReloads:blocks.length,dispatches:dispatches.map((x,j)=>({at:x.at,changed:x.changed,status:responses[j].status})),gap12,gap13,summaryInjectionSha256:sha(JSON.stringify(injected[0].output)),postPasses:stages.slice(1).filter(x=>x.passed).length})
}
assert(contexts.size===1,'Summary injection differs')
for(const pair of ['s1','s2']){const a=slots.filter(x=>x.startsWith(pair));assert(sha(readFileSync(`${work}/runs/${a[0]}/.opencode/skills/zephyr-ledger-audit/SKILL.md`))===sha(readFileSync(`${work}/runs/${a[1]}/.opencode/skills/zephyr-ledger-audit/SKILL.md`)),'Installed skill bytes differ')}
const pin=parse(`${repo}/docs/outreach/artifacts/2026-10-20-omo-full-plugin/results-summary.json`)
assert(sha(readFileSync(`${root}/source/dist/index.js`))===pin.bundleSha256,'Bundle changed')
for(const [path,hash] of Object.entries(pin.sourceHashes))assert(sha(readFileSync(`${root}/source/packages/omo-opencode/src/${path}`))===hash,'Source changed')
writeFileSync(`${work}/analysis.json`,JSON.stringify({arms,controls:{order:conditions,independentMatchedPairs:2,hostConfigsIdentical:true,omoConfigsIdentical:true,nativeDependencyLockHashes:JSON.parse(dependencyLocks!),nativeRuntimeSha256:sha(readFileSync(`${root}/runtime/opencode`)),bun:Bun.version,summaryInjectionOutputsIdentical:true,summaryInjectionSha256:sha([...contexts][0]),recoveryEnvelopeIdentical:true,recoveryEnvelope:JSON.parse(envelope!),nativeConfigRecoveryCounts:[1,1,2],reloadSuccesses:0,model:'openrouter/minimax/minimax-m3.1-flash:free',providerSeed:false,bundleSha256:pin.bundleSha256,sourceHashes:pin.sourceHashes,controllerSha256:sha(readFileSync(`${root}/skill-only-run.ts`)),observerSha256:sha(readFileSync(`${root}/skill-only-observer.ts`))}},null,2))
console.log(JSON.stringify(arms.map(x=>({slot:x.slot,skillText:x.skillText,passes:x.stages.map((s:any)=>s.passed),blocked:x.blockedReloads,gap12:x.gap12,gap13:x.gap13}))))
