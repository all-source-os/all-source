import {readFileSync,writeFileSync,existsSync} from 'node:fs'
const work='/tmp/allsource-omo-compaction/readiness'
const files=['/tmp/oh-my-opencode.log',...Array.from({length:5},(_,i)=>`/tmp/oh-my-opencode.log.${i+1}`)].filter(existsSync)
const lines=files.flatMap(path=>readFileSync(path,'utf8').split('\n'))
for(const slot of ['b1a','b1b','b2a','b2b']){
 const id=JSON.parse(readFileSync(`${work}/evidence/${slot}-identity.json`,'utf8')).subject
 const records=lines.filter(line=>line.includes(id)&&line.includes('[compaction-context-injector]')).map(line=>{
  const match=line.match(/^\[([^\]]+)\] (.+?) (\{.*\})$/)
  if(!match)throw Error('Unexpected native logger format')
  return {at:Date.parse(match[1]),message:match[2],data:JSON.parse(match[3])}
 }).sort((a,b)=>a.at-b.at)
 const captured=records.filter(x=>x.message==='[compaction-context-injector] Captured agent checkpoint before compaction')
 const restored=records.filter(x=>x.message==='[compaction-context-injector] Re-injected checkpointed agent config')
 const failed=records.filter(x=>x.message.includes('Failed to re-inject')||x.message.includes('still incomplete'))
 if(captured.length!==3||restored.length!==2||failed.length)throw Error(`Unexpected native recovery log ${slot}`)
 if(captured.some(x=>x.data.agent!=='build'||x.data.skillCount!==1||!x.data.hasTools||x.data.model.modelID!=='minimax/minimax-m3.1-flash:free'))throw Error(`Checkpoint capture mismatch ${slot}`)
 const second=JSON.parse(readFileSync(`${work}/evidence/${slot}-barrier-2.json`,'utf8'))
 if(!(restored[0].at<captured[1].at&&captured[1].at<=second.eventExitAt&&second.eventExitAt-restored[0].at<60000&&restored[1].at>second.releasedAt))throw Error(`Native cooldown chronology mismatch ${slot}`)
 writeFileSync(`${work}/evidence/${slot}-native-checkpoint-log.json`,JSON.stringify({source:'Native OMO logger; filtered to exact evaluation subject and context-injector only',subject:id,records,captures:3,successfulRestorations:2,cooldownChronologyVerified:true},null,2))
 console.log(JSON.stringify({slot,captures:3,successfulRestorations:2,cooldownChronologyVerified:true}))
}
