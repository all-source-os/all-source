import {appendFileSync} from 'node:fs'
const file=process.env.QA_EVIDENCE!
function log(value:unknown){appendFileSync(file,JSON.stringify({at:Date.now(),...value as object})+'\n')}
export default {id:'allsource-model-omo-observer',async server(input:any){
 const module=await import('/tmp/allsource-omo-compaction/source/dist/index.js')
 const hooks=await module.default.server(input)
 log({kind:'full-plugin-initialized',hooks:Object.keys(hooks),tools:Object.keys(hooks.tool || {})})
 const observed:any={...hooks}
 for(const [name,handler] of Object.entries(hooks))if(typeof handler==='function')observed[name]=async(...args:any[])=>{
  const event=args[0]?.event,tracked=name!=='event'||event?.type==='session.compacted'
  if(tracked)log({kind:'hook-enter',name,sessionID:args[0]?.sessionID,eventType:event?.type,tool:args[0]?.tool,args:name==='tool.execute.before'?args[1]?.args:undefined})
  const result=await (handler as any)(...args)
  if(tracked)log({kind:'hook-exit',name,sessionID:args[0]?.sessionID,eventType:event?.type,tool:args[0]?.tool,outputBytes:name==='tool.execute.after'?Buffer.byteLength(String(args[1]?.output ?? '')):undefined})
  return result
 }
 return observed
}}
