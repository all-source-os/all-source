import {appendFileSync,readFileSync} from 'node:fs'
const evidence=process.env.QA_EVIDENCE || '/tmp/allsource-omo-compaction/full-loader-hooks.jsonl'
function log(value:unknown){appendFileSync(evidence,JSON.stringify({at:Date.now(),...value as object})+'\n')}
log({kind:'module-evaluated'})
export default {id:'allsource-full-omo-observer',async server(input:any){
 try{
  const module=await import('/tmp/allsource-omo-compaction/source/dist/index.js')
  log({kind:'full-module-imported'})
  const hooks=await module.default.server(input)
  log({kind:'full-plugin-initialized',hooks:Object.keys(hooks),tools:Object.keys(hooks.tool || {})})
  const observed:any={...hooks}
  for(const [name,handler] of Object.entries(hooks))if(typeof handler==='function')observed[name]=async(...args:any[])=>{
   const event=args[0]?.event
   const tracked=name!=='event'||event?.type==='session.compacted'
   if(tracked)log({kind:'hook-enter',name,sessionID:args[0]?.sessionID,eventType:event?.type})
   const result=await (handler as any)(...args)
   if(name==='chat.message'&&args[1]?.parts?.some((p:any)=>p.text?.startsWith('QA_LOAD:'))){
    const spec=JSON.parse(readFileSync(`${input.directory}/fixture.json`,'utf8'))
    for(let i=0;i<spec.sizes.length;i++){
     const value=await hooks.tool.skill.execute({name:`qa-${i}`},{sessionID:args[0].sessionID,messageID:args[0].messageID,agent:args[0].agent || 'build',directory:input.directory,worktree:input.worktree,ask:async()=>{},metadata:()=>{},abort:new AbortController().signal})
     log({kind:'full-skill-tool-result',sessionID:args[0].sessionID,name:`qa-${i}`,bytes:Buffer.byteLength(String(value)),endMarker:String(value).includes(`END_${i}`)})
    }
   }
   if(tracked)log({kind:'hook-exit',name,sessionID:args[0]?.sessionID,eventType:event?.type,contextBytes:args[1]?.context?Buffer.byteLength(JSON.stringify(args[1].context)):undefined,enabled:args[1]?.enabled})
   return result
  }
  return observed
 }catch(error){log({kind:'loader-error',error:String(error),stack:(error as Error).stack});throw error}
}}
