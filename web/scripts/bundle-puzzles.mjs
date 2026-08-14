// Bundle the official puzzle corpus (plus their `*-answer` canonical solutions)
// into static JSON under `public/data/`, and emit a `manifest.json` the browser
// uses to lazy-load one puzzle at a time.
//
// Mirrors `src/validation/official_answer.py::_answer_path`: an answer lives in
// a sibling dir whose `ZoneN` component is suffixed `-answer` (e.g.
// `Zone2/10-zone2-mixed/0401.json` → `Zone2-answer/10-zone2-mixed/0401.json`).
// Puzzles outside a `Zone*` dir (A/B/C) have no answer.
//
// Usage: `npm run data` (writes `web/public/data/`).

import { mkdir, readFile, writeFile, readdir, stat } from 'node:fs/promises'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = fileURLToPath(new URL('..', import.meta.url)) // web/
const OFFICIAL = join(ROOT, '..', 'puzzles', 'official')
const OUT = join(ROOT, 'public', 'data')

async function* walk(dir) {
  for (const name of await readdir(dir)) {
    const p = join(dir, name)
    if ((await stat(p)).isDirectory()) yield* walk(p)
    else if (name.endsWith('.json')) yield p
  }
}

function answerPath(puzzlePath) {
  const parts = puzzlePath.split('/')
  for (let i = 0; i < parts.length; i++) {
    if (parts[i].startsWith('Zone') && !parts[i].endsWith('-answer')) {
      return [...parts.slice(0, i), parts[i] + '-answer', ...parts.slice(i + 1)].join('/')
    }
  }
  return null
}

async function readJson(p) {
  try {
    return JSON.parse(await readFile(p, 'utf8'))
  } catch {
    return null
  }
}

async function main() {
  await mkdir(OUT, { recursive: true })
  const manifest = { puzzles: [] }

  for await (const puzzlePath of walk(OFFICIAL)) {
    const rel = relative(OFFICIAL, puzzlePath) // e.g. Zone2/10-zone2-mixed/0401.json
    if (rel.startsWith('Zone') && rel.includes('-answer/')) continue // skip answer trees
    if (rel === '_index.json') continue

    const [zone, ...rest] = rel.split('/')
    const category = rest.length > 1 ? rest.slice(0, -1).join('/') : 'default'
    const id = rel.replace(/\.json$/, '')

    const puzzle = await readJson(puzzlePath)
    if (!puzzle) continue

    let answer = null
    const ansRel = answerPath(rel)
    if (ansRel) {
      const ans = await readJson(join(OFFICIAL, ansRel))
      answer = ans?.regions ?? null
    }

    const entry = { puzzle, answer }
    const outPath = join(OUT, id + '.json')
    await mkdir(dirname(outPath), { recursive: true })
    await writeFile(outPath, JSON.stringify(entry))

    manifest.puzzles.push({
      id,
      zone,
      category,
      url: `data/${id}.json`,
      has_answer: answer != null,
    })
  }

  await writeFile(join(OUT, 'manifest.json'), JSON.stringify(manifest))
  console.log(`bundled ${manifest.puzzles.length} puzzles → ${relative(ROOT, OUT)}/`)
  const withAnswer = manifest.puzzles.filter((p) => p.has_answer).length
  console.log(`  with official answer: ${withAnswer}, solver-only: ${manifest.puzzles.length - withAnswer}`)
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
