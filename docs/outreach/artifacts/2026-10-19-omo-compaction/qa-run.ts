import {mkdirSync,readFileSync,writeFileSync} from 'node:fs'
import {createOpencodeClient} from '@opencode-ai/sdk'
import {createCompactionContextInjector} from './packages/omo-opencode/src/hooks/compaction-context-injector'
import {createSkillTool} from './packages/omo-opencode/src/tools/skill/tools'
import {createSessionCompactingHandler} from './packages/omo-opencode/src/plugin/session-compacting'
const root=process.env.QA_ROOT || '/tmp/allsource-omo-compaction'
const run=process.env.QA_RUN || 'smoke'
const home=`${root}/runs/${run}`
mkdirSync(home,{recursive:true})
const credential=process.env.OPENROUTER_API_KEY || readFileSync('/Users/decebaldobrica/.hermes/.env','utf8').split('\n').find(l=>l.startsWith('OPENROUTER_API_KEY='))?.split('=').slice(1).join('=').trim().replace(/^['"]|['"]$/g,'')
if(!credential) throw Error('OpenRouter credential unavailable')
const config={plugin:[],model:'openrouter/openai/gpt-4.1-mini',provider:{openrouter:{options:{apiKey:'{env:OPENROUTER_API_KEY}'},models:{'openai/gpt-4.1-mini':{limit:{context:1047576,output:1024},options:{maxTokens:1024}}}}},permission:'deny'}
writeFileSync(`${home}/opencode.json`,JSON.stringify(config))
const env={...process.env,HOME:home,XDG_DATA_HOME:`${home}/data`,XDG_CONFIG_HOME:`${home}/config`,XDG_STATE_HOME:`${home}/state`,XDG_CACHE_HOME:`${home}/cache`,OPENCODE_CONFIG:`${home}/opencode.json`,OPENCODE_CONFIG_DIR:`${home}/config`,OPENCODE_DISABLE_PROJECT_CONFIG:'true',OPENROUTER_API_KEY:credential,OPENCODE_DISABLE_MODELS_FETCH:'true'}
delete env.OPENCODE_SERVER_PASSWORD
delete env.OPENCODE_DISABLE_PROJECT_CONFIG
const port=43819
const child=Bun.spawn([process.env.QA_OPENCODE_BIN || `${root}/runtime/opencode`,'--log-level','DEBUG','--print-logs','serve','--hostname','127.0.0.1','--port',String(port)],{cwd:home,env,stdout:Bun.file(`${root}/evidence/${run}-server.log`),stderr:Bun.file(`${root}/evidence/${run}-server.err`)})
const records:any[]=[]
const hookRecords:any[]=[]
const client=createOpencodeClient({baseUrl:`http://127.0.0.1:${port}`})
const injector=createCompactionContextInjector({ctx:{client,directory:home}})
const compact=createSessionCompactingHandler({compactionContextInjector:injector} as any)
const eventAbort=new AbortController()
let eventTask:Promise<void>|undefined
const request=async(path:string,body?:unknown)=>{
 const response=await fetch(`http://127.0.0.1:${port}${path}`,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(90000)})
 const text=await response.text(); let data:any; try{data=JSON.parse(text)}catch{data=text}
 const safe=JSON.parse(JSON.stringify(data,(key,value)=>key==='apiKey'||key==='set-cookie'?'[REDACTED]':value))
 records.push({at:Date.now(),path,status:response.status,data:safe})
 if(!response.ok)throw Error(`HTTP ${response.status}: ${JSON.stringify(data)}`)
 return data
}
try {
 for(let i=0;i<100;i++){try{await request('/global/health');break}catch{await Bun.sleep(200)}}
 await request('/config')
 const eventResponse=await fetch(`http://127.0.0.1:${port}/event`,{signal:eventAbort.signal})
 eventTask=(async()=>{const reader=eventResponse.body!.getReader();let pending='';while(true){const {done,value}=await reader.read();if(done)break;const text=new TextDecoder().decode(value);const file=Bun.file(`${root}/evidence/${run}-events.txt`);await Bun.write(file,(await file.exists()?await file.text():'')+text);pending+=text;const frames=pending.split('\n\n');pending=frames.pop()!;for(const frame of frames){const line=frame.split('\n').find(l=>l.startsWith('data: '));if(!line)continue;const event=JSON.parse(line.slice(6));if(event.type==='session.compacted'){await injector.event({event});hookRecords.push({at:Date.now(),event})}}}})().catch(e=>{if(!eventAbort.signal.aborted)throw e})
 const session=await request('/session',{title:`AllSource #7183 ${run}`})
 const sizes=JSON.parse(process.env.QA_SIZES || '[1024]')
 writeFileSync(`${root}/case.json`,JSON.stringify({sizes}))
 await request(`/session/${session.id}/message`,{agent:'build',model:{providerID:'openrouter',modelID:'openai/gpt-4.1-mini'},noReply:true,parts:[{type:'text',text:'QA_LOAD: synthetic skill boundary fixture. Remember fixture marker.'}]})
 const skills=sizes.map((size:number,i:number)=>{const start=`BEGIN_${i}\n`,end=`\nEND_${i}`;return {name:`qa-${i}`,scope:'config',definition:{description:'Boundary fixture',template:start+'x'.repeat(size-start.length-end.length-(process.env.QA_LEGACY_FIXTURE==='1'?6:0))+end}}})
 const skillTool=createSkillTool({getLoadedSkills:async()=>skills,commands:[],directory:home} as any)
 for(const skill of skills){const result=await skillTool.execute({name:skill.name},{sessionID:session.id,agent:'build',ask:async()=>{},metadata:()=>{},abort:new AbortController().signal} as any);hookRecords.push({kind:'skill-loaded',name:skill.name,bodyBytes:Buffer.byteLength(skill.definition.template),resultBytes:Buffer.byteLength(String(result))})}
 const rounds=Number(process.env.QA_ROUNDS||'1')
 for(let round=0;round<rounds;round++){
  if(round===2)await Bun.sleep(61000)
  const output={context:[]};await compact({sessionID:session.id},output);hookRecords.push({at:Date.now(),kind:'capture',round:round+1,contextBytes:Buffer.byteLength(JSON.stringify(output))})
  await request(`/session/${session.id}/summarize`,{providerID:'openrouter',modelID:'openai/gpt-4.1-mini',auto:false})
  await Bun.sleep(1800)
  const messages=await request(`/session/${session.id}/message`)
  writeFileSync(`${root}/evidence/${run}-round-${round+1}.json`,JSON.stringify({round:round+1,at:Date.now(),sizes,messages},null,2))
 }
} finally {
 eventAbort.abort();await eventTask
 child.kill('SIGTERM'); await Promise.race([child.exited,Bun.sleep(2000)]);if(child.exitCode===null)child.kill('SIGKILL');await child.exited
 writeFileSync(`${root}/evidence/${run}-requests.json`,JSON.stringify(records,null,2))
 writeFileSync(`${root}/evidence/${run}-hooks.json`,JSON.stringify(hookRecords,null,2))
}
