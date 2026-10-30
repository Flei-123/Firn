const fs = require('fs')

function chatFlattener(langPath) {
  const lang = langPath ? JSON.parse(fs.readFileSync(langPath, 'utf8')) : {}
  const format = (template, args) => {
    let next = 0
    return template.replace(/%(?:(\d+)\$)?s|%%/g, (m, idx) => (m === '%%' ? '%' : String(args[idx ? parseInt(idx, 10) - 1 : next++] ?? '')))
  }
  const flat = (j) => {
    try { j = typeof j === 'string' ? JSON.parse(j) : j } catch (e) { return String(j) }
    if (typeof j === 'string' || typeof j === 'number' || typeof j === 'boolean') return String(j)
    if (Array.isArray(j)) return j.map(flat).join('')
    let out = j.text || ''
    if (j.translate) out += format(lang[j.translate] !== undefined ? lang[j.translate] : j.translate, (j.with || []).map(flat))
    return out + (j.extra || []).map(flat).join('')
  }
  return flat
}

module.exports = { chatFlattener }
