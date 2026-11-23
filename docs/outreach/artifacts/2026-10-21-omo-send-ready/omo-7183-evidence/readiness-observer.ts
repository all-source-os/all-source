import {appendFileSync,readFileSync} from 'node:fs'
const file=process.env.QA_EVIDENCE!,policyFile=process.env.QA_POLICY!
function log(value:unknown){appendFileSync(file,JSON.stringify({at:Date.now(),...value as object})+'\n')}
export default {id:'allsource-matched-omo-observer',async server(input:any){
 const module=await import('/tmp/allsource-omo-compaction/source/dist/index.js')
 const nativeFetch=globalThis.fetch
 globalThis.fetch=async function(resource:any,options?:any){
  const request=new Request(resource,options)
  if(request.method!=='POST'||!new URL(request.url).pathname.endsWith('/prompt_async'))return nativeFetch(resource,options)
  const before=await request.clone().json(),after=structuredClone(before)
  const prefix='[restore checkpointed session agent configuration after compaction]'
  const header='[restored skill instructions loaded earlier in this session; re-injected after compaction]'
  const suffix='\n<!-- OMO_INTERNAL_INITIATOR -->\n<!-- OMO_INTERNAL_NOREPLY -->'
  const relevant=(before.parts || []).some((p:any)=>p.text?.startsWith(prefix))
  if(!relevant)return nativeFetch(resource,options)
  let changed=0
  for(const part of after.parts || [])if(part.type==='text'&&part.text.startsWith(prefix)){
   if(!part.text.startsWith(prefix+'\n\n'+header)||!part.text.endsWith(suffix))throw Error('Unexpected recovery skill boundary')
   if(process.env.QA_SKILL_TEXT==='off'){part.text=prefix+suffix;changed++}
  }
  const normalized=(value:any)=>({...value,parts:value.parts.map((p:any)=>p.text?.startsWith(prefix)?{...p,text:prefix+suffix}:p)})
  if(JSON.stringify(normalized(before))!==JSON.stringify(normalized(after)))throw Error('Non-skill recovery fields changed')
  log({kind:'recovery-dispatch-control',skillText:process.env.QA_SKILL_TEXT,path:new URL(request.url).pathname,before,after,changed,nonSkillFieldsIdentical:true})
  const response=await nativeFetch(new Request(request,{body:JSON.stringify(after)}))
  log({kind:'recovery-dispatch-response',status:response.status})
  return response
 } as typeof fetch
 const hooks=await module.default.server(input)
 const loaded=new Set<string>(),observed:any={...hooks,tool:{...hooks.tool}}
 log({kind:'full-plugin-initialized',hooks:Object.keys(hooks),tools:Object.keys(hooks.tool || {})})
 for(const toolName of ['skill','session_search','session_info']){
  const native=hooks.tool[toolName]
  observed.tool[toolName]={...native,execute:async(args:any,ctx:any)=>{
   const policy=JSON.parse(readFileSync(policyFile,'utf8'))
   if(ctx.sessionID===policy.subject){
    if(toolName==='skill'&&loaded.has(ctx.sessionID)){
     log({kind:'reload-blocked',sessionID:ctx.sessionID,args,nativeInvoked:false})
     throw Error('QA policy: skill reload is blocked after the initial successful load.')
    }
    if(toolName!=='skill'&&(args.session_id!==policy.source||(toolName==='session_search'&&!/^ZEPHYR_[A-F0-9]{10}$/.test(args.query || '')))){
     log({kind:'source-policy-blocked',sessionID:ctx.sessionID,tool:toolName,args,nativeInvoked:false})
     throw Error('QA policy: inspect only the requested source with an opaque checkpoint query; broad searches are blocked.')
    }
   }
   const result=await native.execute(args,ctx)
   log({kind:'native-tool-completed',sessionID:ctx.sessionID,tool:toolName,args,outputBytes:Buffer.byteLength(String(result))})
   if(toolName==='skill'&&ctx.sessionID===policy.subject){loaded.add(ctx.sessionID);log({kind:'reload-gate-closed',sessionID:ctx.sessionID})}
   return result
  }}
 }
 for(const [name,handler] of Object.entries(hooks))if(typeof handler==='function')observed[name]=async(...args:any[])=>{
  const event=args[0]?.event,tracked=name!=='event'||event?.type==='session.compacted'
  if(tracked)log({kind:'hook-enter',name,sessionID:args[0]?.sessionID,eventType:event?.type,tool:args[0]?.tool})
  const result=await (handler as any)(...args)
  if(name==='experimental.session.compacting')log({kind:'summary-injection-control',sessionID:args[0]?.sessionID,output:structuredClone(args[1])})
  if(tracked)log({kind:'hook-exit',name,sessionID:args[0]?.sessionID,eventType:event?.type,tool:args[0]?.tool})
  return result
 }
 return observed
}}
