import {appendFileSync,readFileSync,writeFileSync} from 'node:fs'
import {createHash} from 'node:crypto'
export function installProviderCapture(){
 const nativeFetch=globalThis.fetch,file=process.env.QA_EVIDENCE!,policyFile=process.env.QA_POLICY!
 let sequence=0
 const log=(data:object)=>appendFileSync(file,JSON.stringify({at:Date.now(),...data})+'\n')
 function logDev(error:unknown,sequence:number){log({kind:'provider-transport-error',sequence,error:String(error).replace(/sk-or-v1-[A-Za-z0-9]+/g,'[REDACTED]')})}
 globalThis.fetch=async function(resource:any,options?:any){
  const url=new URL(resource instanceof Request?resource.url:String(resource)),method=options?.method??(resource instanceof Request?resource.method:'GET')
  if(url.hostname!=='openrouter.ai'||!url.pathname.endsWith('/chat/completions')||method.toUpperCase()!=='POST')return nativeFetch(resource,options)
  const request=resource instanceof Request?resource.clone():new Request(resource,options),text=await request.text()
  if(/sk-or-v1-[A-Za-z0-9]+|Bearer [A-Za-z0-9_-]{20,}/.test(text))throw Error('Provider body contains credentials; capture stopped before transmission.')
  const body=JSON.parse(text),seq=++sequence,policy=JSON.parse(readFileSync(policyFile,'utf8'))
  const target=file.replace(/-hooks\.jsonl$/,`-provider-${String(seq).padStart(4,'0')}.json`)
  writeFileSync(target,text)
  log({kind:'provider-request',sequence:seq,origin:url.origin,path:url.pathname,bodyFile:target.split('/').at(-1),bodyBytes:Buffer.byteLength(text),bodySha256:createHash('sha256').update(text).digest('hex'),phase:policy.phase??'unassigned',stage:policy.stage??null,subject:policy.subject??null,model:body.model,messageCount:body.messages?.length,stream:body.stream,headersRetained:false,requestUnchanged:true})
  try{
   const response=await nativeFetch(resource,options)
   log({kind:'provider-response',sequence:seq,status:response.status})
   return response
  }catch(error){logDev(error,seq);throw error}
 } as typeof fetch
}
