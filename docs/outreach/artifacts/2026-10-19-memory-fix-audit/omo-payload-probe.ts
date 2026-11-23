import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { readFile } from "node:fs/promises"
import { resolve } from "node:path"
import { pathToFileURL } from "node:url"

const root = process.argv[2]
assert(root, "Pass a checkout or extracted helper directory for PR #7183.")
const sources = [
  {
    path: "packages/omo-opencode/src/hooks/compaction-context-injector/skill-reinjection.ts",
    sha256: "50dcb57f3baa75fdd75c6e176285b6bc24e019db1795a5c45744a53762331b5f",
  },
  {
    path: "packages/omo-opencode/src/shared/session-loaded-skills.ts",
    sha256: "35be0b637313a7ec75392c975dfd0af4474b8a00aac29d7363b05752fbb9bf91",
  },
]
for (const source of sources) {
  const bytes = await readFile(resolve(root, source.path))
  assert.equal(createHash("sha256").update(bytes).digest("hex"), source.sha256,
    `Source differs from audited PR head: ${source.path}`)
}
const formatter = await import(pathToFileURL(resolve(root, sources[0].path)).href)
const store = await import(pathToFileURL(resolve(root, sources[1].path)).href)
const format = formatter.formatCheckpointedSkillsText
assert.equal(typeof format, "function")
assert.equal(typeof store.recordLoadedSkill, "function")
assert.equal(format([]), undefined)

store._resetLoadedSkillsForTesting()
store.recordLoadedSkill("session-a", "team", "version-one")
store.recordLoadedSkill("session-a", "team", "version-two")
assert.deepEqual(store.getSessionLoadedSkills("session-a"), [
  { name: "team", body: "version-two" },
])
assert.deepEqual(store.getSessionLoadedSkills("session-b"), [])
store.clearSessionLoadedSkills("session-a")
assert.deepEqual(store.getSessionLoadedSkills("session-a"), [])

const rows = []
for (const [skillCount, bytesPerSkill] of [[1, 1024], [1, 65536], [1, 1048576], [8, 65536]]) {
  const skills = Array.from({ length: skillCount }, (_, i) => ({
    name: `synthetic-${i}`,
    body: "x".repeat(bytesPerSkill),
  }))
  const rendered = format(skills)
  assert.equal(typeof rendered, "string")
  for (const skill of skills) {
    assert(rendered.includes(`## Skill: ${skill.name}\n\n${skill.body}`))
  }
  const secondRender = format(skills)
  assert.equal(secondRender, rendered)
  rows.push({
    skill_count: skillCount,
    input_body_bytes: skillCount * bytesPerSkill,
    output_bytes: Buffer.byteLength(rendered, "utf8"),
    second_render_bytes: Buffer.byteLength(secondRender, "utf8"),
    all_bodies_preserved: true,
  })
}

process.stdout.write(JSON.stringify({
  source_ref: "93d44a0760448f7f07d2795c6239d51db5ab6576",
  runtime: `bun ${Bun.version}`,
  input: "Synthetic ASCII skill bodies; bytes are not tokens.",
  controls: ["empty input", "latest version replaces prior body", "session isolation", "explicit clear"],
  source_hashes: sources,
  measurements: rows,
  limits: [
    "Runs actual PR formatter and skill store, not OpenCode or recovery event dispatch.",
    "Second render is another pure function call, not a second compaction event.",
    "No accepted budget exists here; this measures growth, not a threshold failure.",
    "No live model, process restart, prompt placement, tokenization, or AllSource integration tested.",
  ],
}, null, 2) + "\n")
