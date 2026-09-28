import { execFileSync } from 'node:child_process'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import { lstat, readdir, readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const workingTree = fileURLToPath(new URL('../..', import.meta.url))
const commonGitDirectory = execFileSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], {
  cwd: workingTree,
  encoding: 'utf8',
}).trim()
const journalRoot = resolve(dirname(commonGitDirectory), '.jev-journal')

function journalFiles(): Plugin {
  return {
    name: 'local-jev-journal-files',
    configureServer(server) {
      server.middlewares.use(async (request, response, next) => {
        const pathname = (request.url ?? '/').split('?', 1)[0]
        if (request.method !== 'GET' || !pathname.startsWith('/api/journal/runs')) {
          next()
          return
        }

        response.setHeader('Cache-Control', 'no-store')
        response.setHeader('X-Content-Type-Options', 'nosniff')
        response.setHeader('Content-Type', 'application/json; charset=utf-8')

        try {
          if (pathname === '/api/journal/runs') {
            let entries
            try {
              const rootInfo = await lstat(journalRoot)
              if (!rootInfo.isDirectory()) {
                response.statusCode = 200
                response.end('[]')
                return
              }
              entries = await readdir(journalRoot, { withFileTypes: true })
            } catch (error) {
              if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT') {
                response.statusCode = 200
                response.end('[]')
                return
              }
              throw error
            }

            const runs = await Promise.all(entries.filter(entry => entry.isDirectory()).map(async entry => {
              const file = resolve(journalRoot, entry.name, 'journal.jsonl')
              if (dirname(file) !== resolve(journalRoot, entry.name)) return null
              try {
                const info = await lstat(file)
                if (!info.isFile()) return null
                return { id: entry.name, modifiedAt: info.mtime.toISOString(), bytes: info.size }
              } catch {
                return null
              }
            }))
            response.statusCode = 200
            response.end(JSON.stringify(runs.filter((run): run is NonNullable<typeof run> => run !== null).sort((a, b) => b.modifiedAt.localeCompare(a.modifiedAt))))
            return
          }

          const match = pathname.match(/^\/api\/journal\/runs\/([^/]+)$/)
          if (!match) {
            response.statusCode = 404
            response.end(JSON.stringify({ error: 'not found' }))
            return
          }
          const id = decodeURIComponent(match[1])
          if (!id || id === '.' || id === '..' || id.includes('/') || id.includes('\\')) {
            response.statusCode = 400
            response.end(JSON.stringify({ error: 'invalid run id' }))
            return
          }
          const runDirectory = resolve(journalRoot, id)
          const file = resolve(runDirectory, 'journal.jsonl')
          if (dirname(file) !== runDirectory || dirname(runDirectory) !== resolve(journalRoot)) {
            response.statusCode = 400
            response.end(JSON.stringify({ error: 'invalid run id' }))
            return
          }
          const directoryInfo = await lstat(runDirectory)
          if (!directoryInfo.isDirectory()) {
            response.statusCode = 404
            response.end(JSON.stringify({ error: 'journal not found' }))
            return
          }
          const info = await lstat(file)
          if (!info.isFile()) {
            response.statusCode = 404
            response.end(JSON.stringify({ error: 'journal not found' }))
            return
          }
          const text = await readFile(file, 'utf8')
          response.statusCode = 200
          response.end(JSON.stringify({ id, text }))
        } catch (error) {
          if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT') {
            response.statusCode = 404
            response.end(JSON.stringify({ error: 'journal not found' }))
            return
          }
          server.config.logger.error(`Could not read local Jev journal: ${error instanceof Error ? error.message : String(error)}`)
          response.statusCode = 500
          response.end(JSON.stringify({ error: 'could not read journal' }))
        }
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), journalFiles()],
  server: {
    watch: {
      ignored: [`${journalRoot}/**`],
    },
  },
})
