import {appendFileSync,readFileSync} from 'node:fs'
const file=process.env.QA_EVIDENCE!,policyFile=process.env.QA_POLICY!
function log(value:unknown){appendFileSync(file,JSON.stringify({at:Date.now(),...value as object})+'\n')}
export default {id:'allsource-matched-omo-observer',async server(input:any){
 const module=await import('/tmp/allsource-omo-compaction/source/dist/index.js')
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
  if(tracked)log({kind:'hook-exit',name,sessionID:args[0]?.sessionID,eventType:event?.type,tool:args[0]?.tool})
  return result
 }
 return observed
}}
