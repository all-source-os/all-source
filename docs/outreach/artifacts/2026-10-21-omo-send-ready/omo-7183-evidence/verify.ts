import {readFileSync,existsSync} from 'node:fs'
import {createHash} from 'node:crypto'
import {resolve} from 'node:path'
const root=process.env.QA_SOURCE_DIR??import.meta.dir,evidence=process.env.QA_EVIDENCE_DIR??resolve(root,'evidence'),parse=(p:string)=>JSON.parse(readFileSync(p,'utf8')),sha=(x:Buffer|string)=>createHash('sha256').update(x).digest('hex')
function assert(x:unknown,message:string){if(!x)throw Error(message)}
const checksums=resolve(root,'CHECKSUMS.json')
if(existsSync(checksums))for(const [file,hash] of Object.entries(parse(checksums)))assert(sha(readFileSync(resolve(root,file)))===hash,`Checksum mismatch ${file}`)
const slots=(process.env.QA_SLOTS??'c1a,c1b,c2a,c2b').split(','),arms:any[]=[]
let requestCount=0,stepCount=0,barrierCount=0,hostConfig:string|undefined,omoConfig:string|undefined
const textOf=(content:any):string=>typeof content==='string'?content:Array.isArray(content)?content.map(x=>typeof x==='string'?x:x?.text??'').join('\n'):''
for(const [index,slot] of slots.entries()){
 const path=(suffix:string)=>resolve(evidence,slot+'-'+suffix),manifest=parse(path('manifest.json')),fixture=parse(resolve(root,manifest.pair+'-shared-fixture.json')),identity=parse(path('identity.json'))
 assert(manifest.skillText===['on','off','off','on'][index],'Counterbalanced order mismatch')
 assert(sha(readFileSync(resolve(root,manifest.pair+'-shared-fixture.json')))===manifest.fixtureSha256,'Fixture hash mismatch')
 const host=JSON.stringify(manifest.config),omo=JSON.stringify(manifest.omo)
 assert(!hostConfig||hostConfig===host,'Host config mismatch');hostConfig=host
 assert(!omoConfig||omoConfig===omo,'OMO config mismatch');omoConfig=omo
 assert(!manifest.omo.disabled_hooks.includes('compaction-context-injector'),'Recovery hook disabled')
 const hooks=readFileSync(path('hooks.jsonl'),'utf8').trim().split('\n').map(x=>JSON.parse(x)),requests=hooks.filter(x=>x.kind==='provider-request'),responses=hooks.filter(x=>x.kind==='provider-response'),dispatches=hooks.filter(x=>x.kind==='recovery-dispatch-control')
 assert(requests.length===responses.length&&!hooks.some(x=>x.kind==='provider-transport-error'),'Provider request/response mismatch')
 assert(hooks.filter(x=>x.kind==='native-tool-completed'&&x.tool==='skill').length===1&&hooks.filter(x=>x.kind==='reload-gate-closed').length===1,'Initial-load latch failed')
 assert(hooks.filter(x=>x.kind==='reload-blocked').every(x=>x.nativeInvoked===false),'Reload bypass')
 assert(dispatches.length===2&&dispatches.every(x=>x.nonSkillFieldsIdentical&&x.changed===(manifest.skillText==='off'?1:0)),'Dispatch-only control failed')
 for(const d of dispatches){const before={...d.before,parts:undefined},after={...d.after,parts:undefined};assert(JSON.stringify(before)===JSON.stringify(after),'Non-skill envelope changed')}
 const payloads=requests.map(r=>{const bodyBytes=readFileSync(resolve(evidence,r.bodyFile)),body=JSON.parse(bodyBytes.toString('utf8'));assert(sha(bodyBytes)===r.bodySha256&&bodyBytes.length===r.bodyBytes,'Provider body mismatch');assert(responses.find(x=>x.sequence===r.sequence)?.status===200,'Provider response not 200');assert(r.requestUnchanged&&r.headersRetained===false&&r.origin==='https://openrouter.ai','Capture transport mismatch');return {...r,body,bodyText:bodyBytes.toString('utf8')}})
 const stages=[]
 for(let stage=0;stage<4;stage++){
  const data=parse(path(`stage-${stage}.json`)),barrier=parse(path(`barrier-${stage}.json`)),ready=parse(path(`ready-messages-${stage}.json`)),calls=data.messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='tool'),expected=fixture.fixtures[stage],sourceID=identity.sources[stage]
  assert(!data.messages.some((m:any)=>m.info.error),'Native model error')
  const search=calls.some((p:any)=>p.tool==='session_search'&&p.state.status==='completed'&&p.state.input.query===expected.query&&p.state.input.session_id===sourceID&&String(p.state.output).includes(expected.receipt))
  const info=calls.some((p:any)=>p.tool==='session_info'&&p.state.status==='completed'&&p.state.input.session_id===sourceID&&String(p.state.output).startsWith('Session ID: '+sourceID))
  const answer=data.messages.filter((m:any)=>m.info.role==='assistant').flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='text').map((p:any)=>p.text).join('\n'),passed=search&&info&&answer.includes(expected.receipt)
  assert(passed===data.score.passed,'Native workflow score mismatch')
  if(stage===0)assert(passed,'Baseline failed');else assert(!calls.some((p:any)=>p.tool==='skill'&&p.state.status==='completed'),'Reload succeeded')
  assert(!data.prompt.includes(expected.query)&&!data.prompt.includes(expected.receipt),'Task leaks answer')
  const task=data.messages.find((m:any)=>m.info.role==='user'&&m.parts.some((p:any)=>p.text===data.prompt))
  assert(task&&task.info.time.created>barrier.releasedAt,'Task preceded barrier')
  const recoveries=ready.messages.filter((m:any)=>m.parts.some((p:any)=>p.text?.startsWith('[restore checkpointed session agent configuration after compaction]')))
  assert(recoveries.length===[0,1,1,2][stage]&&recoveries.every((m:any)=>m.info.time.created<=barrier.releasedAt),'Pre-task recovery missing')
  assert(!data.messages.some((m:any)=>m.parts.some((p:any)=>p.text?.startsWith('[restore checkpointed session agent configuration after compaction]'))),'Late task-turn recovery')
  const group=payloads.filter(x=>x.phase==='checkpoint'&&x.stage===stage),nativeSteps=data.messages.flatMap((m:any)=>m.parts).filter((p:any)=>p.type==='step-start').length
  assert(group.length===nativeSteps&&nativeSteps>0,'Checkpoint capture coverage mismatch')
  const first=group[0],content=first.body.messages.map((m:any)=>textOf(m.content)).join('\n')
  assert(first.at>=task.info.time.created&&!first.bodyText.includes(expected.receipt),'Receipt leaked before source access')
  if(stage===0){assert(!first.bodyText.includes(expected.query),'Baseline query leaked');assert(group.some(x=>x.body.messages.some((m:any)=>m.role==='tool'&&textOf(m.content).includes(fixture.skill.trim()))),'Skill load absent from provider input')}
  stages.push({stage,passed,firstFullSkill:content.includes(fixture.skill.trim()),firstCurrentQuery:first.bodyText.includes(expected.query)})
  requestCount+=group.length;stepCount+=nativeSteps;barrierCount++
 }
 for(let round=1;round<=3;round++){
  const snapshot=parse(path(`compaction-${round}.json`)),summary=snapshot.messages.filter((m:any)=>m.info.summary===true).at(-1),nativeSteps=summary.parts.filter((p:any)=>p.type==='step-start').length,group=payloads.filter(x=>x.phase==='compaction'&&x.stage===round)
  assert(group.length===nativeSteps&&nativeSteps>0,'Compaction capture coverage mismatch');requestCount+=group.length;stepCount+=nativeSteps
 }
 assert(payloads.length===payloads.filter(x=>x.phase==='checkpoint'||x.phase==='compaction').length,'Unclassified provider call')
 arms.push({slot,skillText:manifest.skillText,stages})
}
assert(slots.length===4&&requestCount===stepCount&&barrierCount===16,'Incomplete matched experiment')
console.log(JSON.stringify({verified:true,providerRequests:requestCount,nativeModelSteps:stepCount,barriers:barrierCount,postCompactionPasses:{on:arms.filter(x=>x.skillText==='on').reduce((n,x)=>n+x.stages.slice(1).filter((s:any)=>s.passed).length,0),off:arms.filter(x=>x.skillText==='off').reduce((n,x)=>n+x.stages.slice(1).filter((s:any)=>s.passed).length,0)},arms}))
