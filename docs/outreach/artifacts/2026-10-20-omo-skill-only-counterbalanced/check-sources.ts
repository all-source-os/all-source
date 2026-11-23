import {readFileSync,writeFileSync} from 'node:fs'
const root='/tmp/allsource-omo-compaction/skill-only'
for(const slot of ['s1a','s1b','s2a','s2b']){
 const manifest=JSON.parse(readFileSync(`${root}/evidence/${slot}-manifest.json`,'utf8')),fixture=JSON.parse(readFileSync(`${root}/${manifest.pair}-shared-fixture.json`,'utf8'))
 const home=`${root}/runs/${slot}`,identity=JSON.parse(readFileSync(`${root}/evidence/${slot}-identity.json`,'utf8'))
 const ids=identity.sources.map((x:string)=>`'${x.replaceAll("'","''")}'`).join(',')
 const query=`SELECT session_id,data FROM part WHERE session_id IN (${ids}) ORDER BY session_id`
 const env={PATH:process.env.PATH!,HOME:home,XDG_DATA_HOME:`${home}/data`,XDG_CONFIG_HOME:`${home}/config`,XDG_STATE_HOME:`${home}/state`,XDG_CACHE_HOME:`${home}/cache`,OPENCODE_DISABLE_MODELS_FETCH:'true'}
 const result=Bun.spawnSync(['gtimeout','-k','5','30','/tmp/allsource-omo-compaction/runtime/opencode','--pure','db',query,'--format','json'],{cwd:home,env,stdout:'pipe',stderr:'pipe'})
 if(result.exitCode!==0)throw Error(`Native source query failed ${slot}: ${result.stderr.toString().slice(0,500)}`)
 const rows=JSON.parse(result.stdout.toString())
 const checks=fixture.fixtures.map((x:any,i:number)=>({stage:x.stage,sourceSessionID:identity.sources[i],bodyPresent:rows.some((row:any)=>row.session_id===identity.sources[i]&&String(row.data).includes(x.query)&&String(row.data).includes(x.receipt))}))
 if(checks.some((x:any)=>!x.bodyPresent))throw Error(`Missing original source body ${slot}`)
 writeFileSync(`${root}/evidence/${slot}-source-integrity.json`,JSON.stringify({method:'OpenCode 1.18.22 native db CLI, read-only SELECT, isolated profile',query,rows,checks},null,2))
 console.log(JSON.stringify({slot,sourceBodiesVerified:checks.length}))
}
