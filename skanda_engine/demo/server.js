const express = require('express');
const multer = require('multer');
const cors = require('cors');
const path = require('path');
const fs = require('fs');
const { execFile, spawn } = require('child_process');
const fetch = require('node-fetch');

const app = express();
const PORT = 3000;

app.use(cors());
app.use(express.json());
app.use(express.static(path.join(__dirname, 'public')));

// ─── Paths ────────────────────────────────────────────────────────────────────
const ROOT        = path.join(__dirname, '..');           // skanda_engine root
const RELEASE_DIR = path.join(ROOT, 'target', 'release');
const SKANDA_BIN  = path.join(RELEASE_DIR, 'skanda_engine.exe');
const UPLOADS_DIR = path.join(__dirname, 'uploads');
const INDEX_PATH  = path.join(__dirname, 'data.bin');

if (!fs.existsSync(UPLOADS_DIR)) fs.mkdirSync(UPLOADS_DIR, { recursive: true });

// ─── Multer ───────────────────────────────────────────────────────────────────
const storage = multer.diskStorage({
  destination: UPLOADS_DIR,
  filename: (req, file, cb) => cb(null, file.originalname),
});
const upload = multer({ storage, limits: { fileSize: 50 * 1024 * 1024 } });

// ─── Skanda Bridge State ──────────────────────────────────────────────────────
let skandaProcess = null;
const BRIDGE_PORT = 8181;

function stopSkanda() {
  return new Promise(resolve => {
    if (skandaProcess) {
      skandaProcess.kill();
      skandaProcess = null;
      setTimeout(resolve, 400);
    } else {
      resolve();
    }
  });
}

function startSkanda() {
  return new Promise((resolve, reject) => {
    skandaProcess = spawn(SKANDA_BIN, ['serve', INDEX_PATH, String(BRIDGE_PORT)], {
      stdio: ['ignore', 'pipe', 'pipe']
    });

    let started = false;
    skandaProcess.stdout.on('data', d => {
      const msg = d.toString();
      console.log('[skanda]', msg.trim());
      if (!started && msg.includes('listening')) {
        started = true;
        resolve();
      }
    });
    skandaProcess.stderr.on('data', d => console.error('[skanda]', d.toString().trim()));
    skandaProcess.on('close', code => {
      console.log(`[skanda] exited (${code})`);
      skandaProcess = null;
    });

    // Fallback resolve after 1.5 s even if no stdout
    setTimeout(() => { if (!started) { started = true; resolve(); } }, 1500);
  });
}

function runIndex(dir) {
  return new Promise((resolve, reject) => {
    execFile(SKANDA_BIN, ['index', dir, INDEX_PATH], (err, stdout, stderr) => {
      if (err) return reject(stderr || err.message);
      resolve(stdout);
    });
  });
}

// ─── API: status ──────────────────────────────────────────────────────────────
app.get('/api/status', (req, res) => {
  const files = fs.existsSync(UPLOADS_DIR)
    ? fs.readdirSync(UPLOADS_DIR).filter(f => !f.startsWith('.'))
    : [];
  res.json({
    indexed: fs.existsSync(INDEX_PATH),
    bridgeRunning: !!skandaProcess,
    files,
    bridgePort: BRIDGE_PORT,
  });
});

// ─── API: upload & index ──────────────────────────────────────────────────────
app.post('/api/upload', upload.array('files'), async (req, res) => {
  try {
    if (!req.files || req.files.length === 0)
      return res.status(400).json({ error: 'No files uploaded' });

    // Stop bridge while indexing
    await stopSkanda();

    // Index the uploads directory
    await runIndex(UPLOADS_DIR);

    // Restart bridge with new index
    await startSkanda();

    const files = fs.readdirSync(UPLOADS_DIR).filter(f => !f.startsWith('.'));
    res.json({ success: true, files });
  } catch (err) {
    console.error(err);
    res.status(500).json({ error: String(err) });
  }
});

// ─── API: clear files ─────────────────────────────────────────────────────────
app.post('/api/clear', async (req, res) => {
  try {
    await stopSkanda();
    fs.readdirSync(UPLOADS_DIR).forEach(f => fs.unlinkSync(path.join(UPLOADS_DIR, f)));
    if (fs.existsSync(INDEX_PATH)) fs.unlinkSync(INDEX_PATH);
    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: String(err) });
  }
});

// ─── API: proxy Skanda search ─────────────────────────────────────────────────
app.get('/api/skanda-search', async (req, res) => {
  const { q, fuzzy } = req.query;
  if (!q) return res.status(400).json({ error: 'Missing q' });
  try {
    const url = `http://127.0.0.1:${BRIDGE_PORT}/search?q=${encodeURIComponent(q)}&fuzzy=${fuzzy || 'false'}`;
    const r = await fetch(url);
    const data = await r.json();
    res.json(data);
  } catch (err) {
    res.status(503).json({ error: 'Skanda bridge unavailable — please index files first' });
  }
});

// ─── API: OpenRouter models list ──────────────────────────────────────────────
let modelsCache = null;
let modelsCacheAt = 0;
const MODELS_TTL = 5 * 60 * 1000; // 5 min

app.get('/api/models', async (req, res) => {
  // Serve from cache if fresh
  if (modelsCache && (Date.now() - modelsCacheAt < MODELS_TTL)) {
    return res.json(modelsCache);
  }
  try {
    const headers = { 'Content-Type': 'application/json' };
    const apiKey = req.headers['x-api-key'] || req.query.apiKey;
    if (apiKey) headers['Authorization'] = `Bearer ${apiKey}`;

    const r = await fetch('https://openrouter.ai/api/v1/models', { headers });
    if (!r.ok) throw new Error(`OpenRouter returned ${r.status}`);
    const raw = await r.json();

    // Normalize: pick the fields we care about, sort free first then by name
    const models = (raw.data || []).map(m => ({
      id:          m.id,
      name:        m.name || m.id,
      context:     m.context_length || 0,
      isFree:      (m.id || '').endsWith(':free') ||
                   (m.pricing?.prompt === '0' || m.pricing?.prompt === 0),
      description: m.description || '',
    }));

    models.sort((a, b) => {
      if (a.isFree !== b.isFree) return a.isFree ? -1 : 1;
      return a.name.localeCompare(b.name);
    });

    modelsCache  = { models };
    modelsCacheAt = Date.now();
    res.json(modelsCache);
  } catch (err) {
    console.error('[models]', err.message);
    // Return empty so frontend can show an error
    res.status(502).json({ error: err.message, models: [] });
  }
});

// ─── API: OpenRouter proxy (streaming) ────────────────────────────────────────
app.post('/api/chat', async (req, res) => {
  const { apiKey, model, messages } = req.body;
  if (!apiKey) return res.status(400).json({ error: 'Missing apiKey' });

  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');

  try {
    const orRes = await fetch('https://openrouter.ai/api/v1/chat/completions', {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type':  'application/json',
        'HTTP-Referer':  'http://localhost:3000',
        'X-Title':       'Skanda Demo Chat',
      },
      body: JSON.stringify({ model, messages, stream: true }),
    });

    if (!orRes.ok) {
      const errText = await orRes.text();
      res.write(`data: ${JSON.stringify({ error: errText })}\n\n`);
      return res.end();
    }

    orRes.body.on('data', chunk => res.write(chunk));
    orRes.body.on('end', () => res.end());
    orRes.body.on('error', err => {
      res.write(`data: ${JSON.stringify({ error: err.message })}\n\n`);
      res.end();
    });
  } catch (err) {
    res.write(`data: ${JSON.stringify({ error: err.message })}\n\n`);
    res.end();
  }
});

// ─── Start ─────────────────────────────────────────────────────────────────────
app.listen(PORT, async () => {
  console.log(`🚀  Skanda Demo Chat  →  http://localhost:${PORT}`);
  // Auto-start bridge if an index already exists
  if (fs.existsSync(INDEX_PATH)) {
    console.log('  Found existing index, starting bridge…');
    await startSkanda().catch(console.error);
  }
});

process.on('exit', () => { if (skandaProcess) skandaProcess.kill(); });
process.on('SIGINT', () => { if (skandaProcess) skandaProcess.kill(); process.exit(); });
