// Headless capture of the mobile Jarvis web app (run by capture.py).
// usage: node capture.mjs <baseUrl> <outDir> <dprList e.g. 1,3> <themes e.g. fleitec,claude> [screens]
import { createRequire } from 'module'
const require = createRequire('/root/jarvis/node_modules/')
const { chromium } = require('playwright')
const [base, out, dprArg, themeArg, screenArg] = process.argv.slice(2)
const dprs = (dprArg || '1').split(',').map(Number)
const themes = (themeArg || 'fleitec').split(',')
const want = screenArg ? screenArg.split(',') : null
const W = 412, H = 915

const screens = {
  login: async (p) => { await p.goto(base + '/login', { waitUntil: 'load' }) },
  chat: async (p) => { await p.goto(base + '/?c=c1', { waitUntil: 'load' }); await p.waitForSelector('.msg', { timeout: 8000 }) },
  board: async (p) => { await p.goto(base + '/', { waitUntil: 'load' }); await p.waitForSelector('.pj-grid .pj-card', { timeout: 8000 }) },
  needs: async (p) => {
    await p.goto(base + '/', { waitUntil: 'load' }); await p.waitForSelector('.pj-needbox', { timeout: 8000 })
    await p.evaluate(() => document.querySelector('.pj-needbox').scrollIntoView({ block: 'start' }))
  },
  tasks: async (p) => {
    await p.goto(base + '/?c=c1', { waitUntil: 'load' }); await p.waitForSelector('.msg', { timeout: 8000 })
    await p.click('#chatSettingsBtn'); await p.click('#tasksBtn'); await p.waitForSelector('.task-item', { timeout: 5000 })
  },
  devices: async (p) => {
    await p.goto(base + '/?c=c-orch', { waitUntil: 'load' }); await p.waitForSelector('#hudDevices .cc-row', { timeout: 8000 })
    await p.evaluate(() => document.getElementById('hudDevices').closest('.cc-card').scrollIntoView({ block: 'center' }))
  },
  settings: async (p) => {
    await p.goto(base + '/?c=c1', { waitUntil: 'load' }); await p.waitForSelector('.msg', { timeout: 8000 })
    await p.evaluate(() => document.getElementById('settings').click()); await p.waitForSelector('#settingsModal:not(.hidden)', { timeout: 5000 })
  },
}

const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', args: ['--no-sandbox'] })
for (const dpr of dprs) {
  const unknown = []
  for (const name of Object.keys(screens)) {
    if (want && !want.includes(name)) continue
    for (const theme of themes) {
      // fresh context per shot: the app remembers the last open chat in localStorage
      const ctx = await browser.newContext({ viewport: { width: W, height: H }, deviceScaleFactor: dpr, isMobile: true, hasTouch: true, locale: 'de-AT', timezoneId: 'Europe/Vienna' })
      const page = await ctx.newPage()
      page.on('requestfailed', (r) => { if (!/websocket/i.test(r.url())) unknown.push(r.url()) })
      await page.addInitScript(() => { window.WebSocket = class { constructor() { this.readyState = 3; setTimeout(() => this.onerror && this.onerror(), 10) } send() {} close() {} } })
      try {
        await screens[name](page)
        await page.evaluate((t) => { document.documentElement.dataset.theme = t; const b = document.getElementById('connBar'); if (b) b.remove(); const s = document.createElement('style'); s.textContent = '#connBar{display:none!important} *{caret-color:transparent!important;animation:none!important;transition:none!important}'; document.head.appendChild(s) }, theme)
        await page.waitForTimeout(500)
        const label = theme === 'fleitec' ? 'dark' : theme
        const f = `${out}/${name}-${label}${dpr === 1 ? '' : '@' + dpr + 'x'}.png`
        await page.screenshot({ path: f })
        console.log('wrote', f)
      } catch (e) { console.log('FAIL', name, theme, dpr, String(e).split('\n')[0]) }
      await ctx.close()
    }
  }
}
await browser.close()
