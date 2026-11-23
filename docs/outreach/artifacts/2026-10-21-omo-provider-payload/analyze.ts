import {readFileSync,writeFileSync} from 'node:fs'
import {createHash} from 'node:crypto'
const root='/tmp/allsource-omo-compaction',work=`${root}/provider`,repo='/Users/decebaldobrica/Projects/founder-mode/all-source'
const parse=(path:string)=>JSON.parse(readFileSync(path,'utf8')),sha=(x:string|Buffer)=>createHash('sha256').update(x).digest('hex')
function assert(ok:unknown,message:string){if(!ok)throw Error(message)}
const slots=['c1a','c1b','c2a','c2b'],conditions=['on','off','off','on'],arms:any[]=[],contexts=new Set<string>()
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
 const readiness=[]
 for(let stage=0;stage<4;stage++){
  const barrier=parse(work+'/evidence/'+slot+'-barrier-'+stage+'.json'),ready=parse(work+'/evidence/'+slot+'-ready-messages-'+stage+'.json'),turn=parse(work+'/evidence/'+slot+'-stage-'+stage+'.json')
  const task=turn.messages.find((m:any)=>m.info.role==='user'&&m.parts.some((p:any)=>p.type==='text'&&p.text===turn.prompt))
  assert(task&&barrier.releasedAt<task.info.time.created,'Task started before readiness release')
  assert(barrier.expectedRecoveries===[0,1,1,2][stage]&&barrier.state===['initial-ready','accepted-and-stored','settled-cooldown-suppressed','accepted-and-stored'][stage],'Wrong barrier state')
  const last=barrier.polls.at(-1)
  assert(last.idle&&last.matched&&last.initialized&&last.latch&&last.regime,'Readiness control failed')
  assert(last.enters===stage&&last.exits===stage&&last.summaries===stage&&last.stored===[0,1,1,2][stage]&&last.dispatches===last.stored&&last.responses===last.stored,'Barrier count mismatch')
  assert(barrier.elapsedMs>=0&&barrier.elapsedMs<20000,'Barrier exceeded deadline')
  if(stage>0)assert(barrier.eventExitAt<=barrier.releasedAt&&barrier.acceptedAt<=barrier.releasedAt,'Barrier released before hook/acceptance')
  const recovery=ready.messages.filter((m:any)=>m.parts.some((p:any)=>p.synthetic&&p.text?.startsWith('[restore checkpointed session agent configuration after compaction]')))
  assert(recovery.length===[0,1,1,2][stage]&&recovery.every((m:any)=>m.info.time.created<=barrier.releasedAt),'Recovery not stored before barrier')
  assert(!turn.messages.some((m:any)=>m.parts.some((p:any)=>p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))),'Recovery arrived during task turn')
  readiness.push({stage,state:barrier.state,elapsedMs:barrier.elapsedMs,polls:barrier.polls.length,releasedAt:barrier.releasedAt,taskCreatedAt:task.info.time.created,leadMs:task.info.time.created-barrier.releasedAt,storedRecoveryCount:recovery.length,eventExitAt:barrier.eventExitAt,acceptedAt:barrier.acceptedAt})
 }
 const compactions=[]
 for(let round=1;round<=3;round++){
  const d=parse(`${work}/evidence/${slot}-compaction-${round}.json`),summaries=d.messages.filter((m:any)=>m.info.summary===true)
  assert(summaries.length===round&&!d.messages.some((m:any)=>m.info.error),'Native summary failed')
  const rawRecovery=d.messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.synthetic&&p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))
  const turn=parse(`${work}/evidence/${slot}-stage-${round}.json`),byID=new Map([...d.messages,...turn.messages].map((m:any)=>[m.info.id,m]))
  const recovery=[...byID.values()].flatMap((m:any)=>m.parts).filter((p:any)=>p.synthetic&&p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))
  assert(recovery.length===[1,1,2][round-1]&&rawRecovery.length===recovery.length,'Stored config recovery count mismatch')
  assert(recovery.every((p:any)=>p.text.includes('END_ZEPHYR_SKILL')===(conditions[i]==='on')),'Stored skill treatment mismatch')
  const late=dispatches.filter(x=>x.at>d.at&&x.at<turn.at);assert(late.length===0,'Late dispatch after ready snapshot')
  const taskMessage=turn.messages.find((m:any)=>m.info.role==='user'&&m.parts.some((p:any)=>p.type==='text'&&p.text===turn.prompt))
  const duringTurn=turn.messages.filter((m:any)=>m.parts.some((p:any)=>p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))).map((m:any)=>({createdAt:m.info.time.created,afterTaskMessageMs:m.info.time.created-taskMessage.info.time.created}))
  compactions.push({round,rawSnapshotConfigRecoveries:rawRecovery.length,configRecoveries:recovery.length,skillRecoveries:recovery.filter((p:any)=>p.text.includes('END_ZEPHYR_SKILL')).length,lateDispatches:late.map(x=>({at:x.at,afterSnapshotMs:x.at-d.at})),recoveryMessagesDuringTurn:duringTurn,summary:summaries.at(-1).parts.filter((p:any)=>p.type==='text').map((p:any)=>p.text).join('\n')})
 }
 arms.push({slot,pair:manifest.pair,skillText:manifest.skillText,stages,compactions,readiness,blockedReloads:blocks.length,dispatches:dispatches.map((x,j)=>({at:x.at,changed:x.changed,status:responses[j].status})),gap12,gap13,summaryInjectionSha256:sha(JSON.stringify(injected[0].output)),postPasses:stages.slice(1).filter(x=>x.passed).length})
}
assert(contexts.size===1,'Summary injection differs')
for(const pair of ['c1','c2']){const a=slots.filter(x=>x.startsWith(pair));assert(sha(readFileSync(`${work}/runs/${a[0]}/.opencode/skills/zephyr-ledger-audit/SKILL.md`))===sha(readFileSync(`${work}/runs/${a[1]}/.opencode/skills/zephyr-ledger-audit/SKILL.md`)),'Installed skill bytes differ')}
const pin=parse(`${repo}/docs/outreach/artifacts/2026-10-20-omo-full-plugin/results-summary.json`)
assert(sha(readFileSync(`${root}/source/dist/index.js`))===pin.bundleSha256,'Bundle changed')
for(const [path,hash] of Object.entries(pin.sourceHashes))assert(sha(readFileSync(`${root}/source/packages/omo-opencode/src/${path}`))===hash,'Source changed')
writeFileSync(`${work}/analysis.json`,JSON.stringify({arms,controls:{order:conditions,independentMatchedPairs:2,hostConfigsIdentical:true,omoConfigsIdentical:true,readinessBarriersVerified:16,lateRecoveryMessages:0,nativeDependencyLockHashes:JSON.parse(dependencyLocks!),nativeRuntimeSha256:sha(readFileSync(`${root}/runtime/opencode`)),bun:Bun.version,summaryInjectionOutputsIdentical:true,summaryInjectionSha256:sha([...contexts][0]),recoveryEnvelopeIdentical:true,recoveryEnvelope:JSON.parse(envelope!),nativeConfigRecoveryCounts:[1,1,2],reloadSuccesses:0,model:'openrouter/minimax/minimax-m3.1-flash:free',providerSeed:false,bundleSha256:pin.bundleSha256,sourceHashes:pin.sourceHashes,controllerSha256:sha(readFileSync(`${root}/provider-run.ts`)),observerSha256:sha(readFileSync(`${root}/provider-observer.ts`)),baselineObserverSha256:sha(readFileSync(`${root}/readiness-observer.ts`)),providerCaptureSha256:sha(readFileSync(`${root}/provider-capture.ts`))}},null,2))
const previous=parse(repo+'/docs/outreach/artifacts/2026-10-20-omo-skill-only-counterbalanced/results-summary.json')
assert(sha(readFileSync(root+'/readiness-observer.ts'))===previous.controls.observerSha256,'Observer changed from prior skill-only run')
for(const pair of ['c1','c2'])assert(sha(readFileSync(work+'/'+pair+'-shared-fixture.json'))===sha(readFileSync(root+'/skill-only/'+pair.replace('c','s')+'-shared-fixture.json')),'Prior fixture changed')
const barrierBody=(s:string)=>s.slice(s.indexOf('async function recoveryBarrier'),s.indexOf('\ntry{'))
assert(sha(barrierBody(readFileSync(root+'/provider-run.ts','utf8')))===sha(barrierBody(readFileSync(root+'/readiness-run.ts','utf8'))),'Readiness barrier implementation changed')
console.log(JSON.stringify(arms.map(x=>({slot:x.slot,skillText:x.skillText,passes:x.stages.map((s:any)=>s.passed),blocked:x.blockedReloads,gap12:x.gap12,gap13:x.gap13}))))
