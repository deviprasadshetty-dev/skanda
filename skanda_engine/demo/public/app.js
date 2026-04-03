/* ════════════════════════════════════════════════
   SKANDA CHAT — app.js
   Full Skanda RAG loop:
     1. User types question
     2. LLM generates footprints (Skanda tool call)
     3. Skanda retrieves SIMD-matched snippets
     4. LLM answers grounded in retrieved context
   ════════════════════════════════════════════════ */

'use strict';

// ─── State ────────────────────────────────────────────────────────────────────
const state = {
  files:         [],
  indexed:       false,
  bridgeOnline:  false,
  apiKey:        localStorage.getItem('sk_api_key') || '',
  model:         localStorage.getItem('sk_model')   || 'google/gemma-3-27b-it:free',
  fuzzy:         localStorage.getItem('sk_fuzzy') !== 'false',
  history:       [],          // [{role, content}]
  streaming:     false,
  lastSnippets:  [],
};

// ─── DOM refs ─────────────────────────────────────────────────────────────────
const $ = id => document.getElementById(id);
const apiKeyEl      = $('apiKey');
const toggleKey     = $('toggleKey');
const modelSelect   = $('modelSelect');
const modelSearch   = $('modelSearch');
const modelCount    = $('modelCount');
const refreshModels = $('refreshModels');
const fuzzyToggle   = $('fuzzyToggle');
const dropZone      = $('dropZone');
const fileInput     = $('fileInput');
const uploadBtn     = $('uploadBtn');
const fileList      = $('fileList');
const indexProgress = $('indexProgress');
const progressFill  = $('progressFill');
const progressLabel = $('progressLabel');
const statusDot     = $('statusDot');
const statusText    = $('statusText');
const clearBtn      = $('clearBtn');
const messages      = $('messages');
const welcomeScreen = $('welcomeScreen');
const userInput     = $('userInput');
const sendBtn       = $('sendBtn');
const bridgeBadge   = $('bridgeBadge');
const retrievalCounter = $('retrievalCounter');
const rcValue       = $('rcValue');
const retrievalDrawer  = $('retrievalDrawer');
const rdBody        = $('rdBody');
const rdClose       = $('rdClose');
const attachBtn     = $('attachBtn');
const sidebar       = $('sidebar');
const sidebarToggle = $('sidebarToggle');
const toast         = $('toast');

// ─── Init UI from state ───────────────────────────────────────────────────────
apiKeyEl.value       = state.apiKey;
fuzzyToggle.checked  = state.fuzzy;

// ─── Persist preferences ──────────────────────────────────────────────────────
apiKeyEl.addEventListener('input', () => {
  state.apiKey = apiKeyEl.value.trim();
  localStorage.setItem('sk_api_key', state.apiKey);
  updateSendBtn();
  // Re-fetch models with/without auth when key changes
  clearTimeout(apiKeyEl._refetchTimer);
  apiKeyEl._refetchTimer = setTimeout(() => fetchModels(true), 800);
});
modelSelect.addEventListener('change', () => {
  state.model = modelSelect.value;
  localStorage.setItem('sk_model', state.model);
});
fuzzyToggle.addEventListener('change', () => {
  state.fuzzy = fuzzyToggle.checked;
  localStorage.setItem('sk_fuzzy', String(state.fuzzy));
});
toggleKey.addEventListener('click', () => {
  apiKeyEl.type = apiKeyEl.type === 'password' ? 'text' : 'password';
});

// ─── Dynamic model list ───────────────────────────────────────────────────────
let _allModels = [];   // cache for search filter

async function fetchModels(force = false) {
  refreshModels.classList.add('spinning');
  modelSelect.disabled = true;
  try {
    const url = force ? '/api/models?bust=' + Date.now() : '/api/models';
    const headers = {};
    if (state.apiKey) headers['x-api-key'] = state.apiKey;
    const r = await fetch(url, { headers });
    const d = await r.json();
    if (d.error && !d.models.length) throw new Error(d.error);
    _allModels = d.models || [];
    renderModelSelect(_allModels, state.model, modelSearch.value);
    modelCount.textContent = _allModels.length + ' models';
    if (force) showToast(`Loaded ${_allModels.length} models from OpenRouter`, 'success');
  } catch (e) {
    showToast('Failed to load models: ' + e.message, 'error');
    // keep any existing options
  } finally {
    refreshModels.classList.remove('spinning');
    modelSelect.disabled = false;
  }
}

function renderModelSelect(models, selectedId, filterText = '') {
  const q = (filterText || '').toLowerCase();
  const filtered = q
    ? models.filter(m => m.name.toLowerCase().includes(q) || m.id.toLowerCase().includes(q))
    : models;

  const freeModels = filtered.filter(m => m.isFree);
  const paidModels = filtered.filter(m => !m.isFree);

  modelSelect.innerHTML = '';

  function addGroup(label, items) {
    if (!items.length) return;
    const grp = document.createElement('optgroup');
    grp.label = label;
    items.forEach(m => {
      const opt = document.createElement('option');
      opt.value = m.id;
      const ctx = m.context >= 1000 ? Math.round(m.context / 1000) + 'k ctx' : '';
      opt.textContent = m.name + (ctx ? ` — ${ctx}` : '');
      if (m.id === selectedId) opt.selected = true;
      grp.appendChild(opt);
    });
    modelSelect.appendChild(grp);
  }

  addGroup('🆓 Free Models (' + freeModels.length + ')', freeModels);
  addGroup('💳 Paid Models (' + paidModels.length + ')', paidModels);

  if (!modelSelect.value && filtered.length) {
    modelSelect.value = filtered[0].id;
  }
  state.model = modelSelect.value;
}

// Search filter
modelSearch.addEventListener('input', () => {
  renderModelSelect(_allModels, state.model, modelSearch.value);
});

// Refresh button
refreshModels.addEventListener('click', () => fetchModels(true));

// ─── Sidebar toggle ───────────────────────────────────────────────────────────
sidebarToggle.addEventListener('click', () => {
  sidebar.classList.toggle('collapsed');
});

// ─── Toast ────────────────────────────────────────────────────────────────────
let toastTimer;
function showToast(msg, type = 'info', ms = 3000) {
  toast.textContent = msg;
  toast.className = `toast show ${type}`;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { toast.className = 'toast'; }, ms);
}

// ─── Status polling ───────────────────────────────────────────────────────────
async function pollStatus() {
  try {
    const r = await fetch('/api/status');
    if (!r.ok) return;
    const d = await r.json();
    state.indexed      = d.indexed;
    state.bridgeOnline = d.bridgeRunning;
    state.files        = d.files || [];
    updateStatusUI();
    updateSendBtn();
  } catch { /* server not ready yet */ }
}

function updateStatusUI() {
  if (state.bridgeOnline) {
    // ✅ Fully online — files indexed, bridge serving
    bridgeBadge.textContent = '⚡ bridge ready';
    bridgeBadge.classList.add('online');
    statusDot.className = 'status-dot online';
    statusText.textContent = `${state.files.length} file${state.files.length !== 1 ? 's' : ''} indexed`;
  } else if (state.indexed) {
    // ⚠️ Index exists but bridge process stopped (real fault)
    bridgeBadge.textContent = '● bridge offline';
    bridgeBadge.classList.remove('online');
    statusDot.className = 'status-dot error';
    statusText.textContent = 'Bridge stopped — re-upload to restart';
  } else {
    // ℹ️ Nothing indexed yet — this is the normal first-run state
    bridgeBadge.textContent = '○ awaiting files';
    bridgeBadge.classList.remove('online');
    statusDot.className = 'status-dot';
    statusText.textContent = 'Drop files above to index';
  }
  // File list & clear button
  renderFileList(state.files);
  clearBtn.hidden = state.files.length === 0;
}


function renderFileList(files) {
  fileList.innerHTML = '';
  files.forEach(name => {
    const ext  = name.split('.').pop() || '?';
    const li   = document.createElement('li');
    li.className = 'file-item';
    li.innerHTML = `<span class="file-ext">${ext}</span><span class="file-name" title="${name}">${name}</span>`;
    fileList.appendChild(li);
  });
}

setInterval(pollStatus, 4000);
pollStatus();

// ─── Drag & Drop ──────────────────────────────────────────────────────────────
dropZone.addEventListener('dragover', e => { e.preventDefault(); dropZone.classList.add('drag-over'); });
dropZone.addEventListener('dragleave', ()  => dropZone.classList.remove('drag-over'));
dropZone.addEventListener('drop', e => {
  e.preventDefault();
  dropZone.classList.remove('drag-over');
  handleFiles(Array.from(e.dataTransfer.files));
});
uploadBtn.addEventListener('click', () => fileInput.click());
fileInput.addEventListener('change', () => handleFiles(Array.from(fileInput.files)));

async function handleFiles(files) {
  if (!files.length) return;
  const allowed = ['txt','md','csv','json','rs','py','js'];
  const filtered = files.filter(f => {
    const ext = f.name.split('.').pop().toLowerCase();
    return allowed.includes(ext);
  });
  if (!filtered.length) { showToast('Unsupported file type(s). Allowed: txt md csv json rs py js', 'error'); return; }

  // Show progress
  indexProgress.hidden = false;
  progressFill.style.width = '0%';
  progressLabel.textContent = `Uploading ${filtered.length} file(s)…`;
  statusDot.className = 'status-dot indexing';
  statusText.textContent = 'Indexing…';

  let pct = 0;
  const fillInterval = setInterval(() => {
    pct = Math.min(pct + 3, 85);
    progressFill.style.width = pct + '%';
  }, 80);

  const fd = new FormData();
  filtered.forEach(f => fd.append('files', f));

  try {
    const r = await fetch('/api/upload', { method: 'POST', body: fd });
    clearInterval(fillInterval);

    if (r.ok) {
      const d = await r.json();
      progressFill.style.width = '100%';
      progressLabel.textContent = '✓ Index built!';
      state.files   = d.files;
      state.indexed = true;
      updateStatusUI();
      showToast(`⚡ Indexed ${filtered.length} file(s). Skanda bridge is live!`, 'success', 4000);
      setTimeout(() => { indexProgress.hidden = true; }, 2000);
    } else {
      const err = await r.json();
      throw new Error(err.error || 'Unknown error');
    }
  } catch (e) {
    clearInterval(fillInterval);
    progressFill.style.width = '0%';
    progressLabel.textContent = 'Error — check console';
    showToast('Upload failed: ' + e.message, 'error');
    statusDot.className = 'status-dot error';
    statusText.textContent = 'Failed';
    setTimeout(() => { indexProgress.hidden = true; }, 3000);
  }
}

// ─── Clear ────────────────────────────────────────────────────────────────────
clearBtn.addEventListener('click', async () => {
  if (!confirm('Clear all uploaded files and index?')) return;
  try {
    await fetch('/api/clear', { method: 'POST' });
    state.files = []; state.indexed = false; state.bridgeOnline = false;
    updateStatusUI();
    showToast('Files cleared', 'info');
  } catch(e) { showToast('Clear failed: ' + e.message, 'error'); }
});

// ─── Retrieval drawer ──────────────────────────────────────────────────────────
rdClose.addEventListener('click', () => { retrievalDrawer.hidden = true; });
attachBtn.addEventListener('click', () => {
  if (!state.lastSnippets.length) { showToast('No retrieval results yet', 'info'); return; }
  retrievalDrawer.hidden = !retrievalDrawer.hidden;
  if (!retrievalDrawer.hidden) renderRetrievalDrawer(state.lastSnippets);
});

function renderRetrievalDrawer(snippets) {
  rdBody.innerHTML = '';
  if (!snippets.length) { rdBody.innerHTML = '<p style="color:var(--text-3);font-size:12px">No snippets retrieved.</p>'; return; }
  snippets.forEach((s, i) => {
    const div = document.createElement('div');
    div.className = 'rd-snippet';
    const parts = s.file_path.replace(/\\/g, '/').split('/');
    const fname = parts[parts.length - 1];
    div.innerHTML = `<div class="rd-file">[${i+1}] ${fname}</div><div class="rd-text">${escapeHtml(s.snippet)}</div>`;
    rdBody.appendChild(div);
  });
}

// ─── Text auto-resize ──────────────────────────────────────────────────────────
userInput.addEventListener('input', () => {
  userInput.style.height = 'auto';
  userInput.style.height = Math.min(userInput.scrollHeight, 180) + 'px';
  updateSendBtn();
});
userInput.addEventListener('keydown', e => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
});

function updateSendBtn() {
  sendBtn.disabled = !userInput.value.trim() || !state.apiKey || state.streaming;
}

function insertPrompt(text) {
  userInput.value = text;
  userInput.style.height = 'auto';
  userInput.style.height = Math.min(userInput.scrollHeight, 180) + 'px';
  updateSendBtn();
  userInput.focus();
}
window.insertPrompt = insertPrompt;

// ─── MAIN: Send message ────────────────────────────────────────────────────────
sendBtn.addEventListener('click', sendMessage);

async function sendMessage() {
  const text = userInput.value.trim();
  if (!text || !state.apiKey || state.streaming) return;

  // Hide welcome
  welcomeScreen.hidden = true;

  // Append user bubble
  appendMsg('user', text);
  state.history.push({ role: 'user', content: text });

  userInput.value = '';
  userInput.style.height = 'auto';
  state.streaming = true;
  updateSendBtn();

  // Show thinking
  const thinkingEl = appendThinking();

  try {
    let snippets = [];

    if (state.indexed && state.bridgeOnline) {
      // ═══════════════════════════════════════════════════════
      //  SKANDA RAG LOOP
      //  Phase 1: Ask LLM to predict footprints
      // ═══════════════════════════════════════════════════════
      const footprintMessages = [
        {
          role: 'system',
          content: `You are a search query optimizer for Skanda Engine — an exact-match, SIMD-accelerated retrieval system.
Your only job: given a user question about uploaded text documents, predict 3-8 rare, specific words that are LIKELY to appear VERBATIM in the source text near the answer.

Rules:
- Prefer: proper nouns, character names, place names, unusual verbs, specific objects, direct speech fragments, numbers.
- Avoid: common words (the, a, is, was, in, of, for, that).
- Think about what a real sentence in the text would look like, then pick its rarest words.
- Output ONLY a JSON object with one key "footprints". Nothing else.

Example: {"footprints": "Sitka snow baby Allen Chaffee cradle fur"}`
        },
        {
          role: 'user',
          content: `Question: ${text}`
        }
      ];

      let footprintJson = '';
      try {
        footprintJson = await completeLLM(footprintMessages);
      } catch (e) {
        console.warn('Footprint generation failed, falling back to direct query:', e);
      }

      let footprintQuery = text; // fallback
      if (footprintJson) {
        try {
          // Extract JSON from possible markdown fence
          const raw = footprintJson.replace(/```json\s*/gi,'').replace(/```/g,'').trim();
          const parsed = JSON.parse(raw);
          if (parsed.footprints) footprintQuery = parsed.footprints;
        } catch {
          // Regex fallback
          const m = footprintJson.match(/"footprints"\s*:\s*"([^"]+)"/);
          if (m) footprintQuery = m[1];
        }
      }

      console.log('[Skanda] footprints:', footprintQuery);

      // ═══════════════════════════════════════════════════════
      //  Phase 2: Skanda retrieval
      // ═══════════════════════════════════════════════════════
      try {
        const url = `/api/skanda-search?q=${encodeURIComponent(footprintQuery)}&fuzzy=${state.fuzzy}`;
        const sr = await fetch(url);
        if (sr.ok) {
          snippets = await sr.json();
          state.lastSnippets = snippets;
          // Update retrieval counter
          rcValue.textContent = snippets.length;
          retrievalCounter.hidden = false;
          console.log(`[Skanda] retrieved ${snippets.length} snippets`);
        }
      } catch (e) {
        console.warn('[Skanda] retrieval failed:', e);
      }
    }

    // ═══════════════════════════════════════════════════════
    //  Phase 3: Final LLM answer with retrieved context
    // ═══════════════════════════════════════════════════════
    let systemPrompt;

    if (snippets.length > 0) {
      const ctx = snippets.map((s, i) => {
        const parts = s.file_path.replace(/\\/g, '/').split('/');
        const fname = parts[parts.length - 1];
        return `[EXCERPT ${i+1}] (from: ${fname})\n${s.snippet}`;
      }).join('\n\n---\n\n');

      systemPrompt = `You are a precise, evidence-bound reading assistant. Your answers must be fully grounded in the RETRIEVED EXCERPTS below.

════════════════════════════════════════════
RETRIEVED EXCERPTS (your ONLY knowledge source for this query):
════════════════════════════════════════════
${ctx}
════════════════════════════════════════════

STRICT BEHAVIORAL RULES — follow every rule without exception:

1. CONTEXT-FIRST: Use ONLY the retrieved excerpts above as your knowledge source. Do not introduce facts, descriptions, or interpretations from general world knowledge.

2. EVIDENCE BINDING: Every claim in your response must be directly supported by a sentence in the excerpts. If a claim is not supported, write: "Not stated in the text."

3. QUOTE VERIFICATION: You may only output direct quotations that appear VERBATIM in the excerpts above. Do not paraphrase and present it as a quote. If you cannot find exact wording, describe the content and note it is paraphrased, or write "Not stated in the text."

4. CONTROLLED INTERPRETATION: You may offer analysis or interpretation ONLY when it is explicitly tied to a specific passage in the excerpts. Do not introduce symbolic, ecological, mythological, or thematic readings that are not grounded in the text.

5. FACT ANCHORING: Extract character identities, roles, genders, relationships, and core facts directly from the text. Never infer or guess. If a fact is ambiguous or absent, say so.

6. OMISSION OVER SPECULATION: If information is not present in the excerpts, state clearly: "Not stated in the text." Do not fill gaps with plausible-sounding details.

7. CITATION: When stating a fact, reference which excerpt number it came from, e.g. "[EXCERPT 2]".

Respond clearly and concisely. Distinguish explicitly between supported facts and any inferences you draw.`;

    } else if (state.indexed) {
      systemPrompt = `You are a reading assistant for documents the user has uploaded.

Skanda Engine attempted a retrieval but found no matching excerpts for this query.

Tell the user:
1. That no relevant passages were found for their question.
2. Suggest they rephrase with more specific or unusual words from the text (character names, place names, rare terms).
3. Do NOT answer the question from general knowledge — you have no verified source for this document.`;

    } else {
      systemPrompt = `You are a reading assistant. No documents have been uploaded or indexed yet.

Politely tell the user to upload their files using the sidebar, then ask their question again. Do not answer questions about document content without a loaded index.`;
    }

    const finalMessages = [
      { role: 'system', content: systemPrompt },
      ...state.history.slice(-10),   // last 10 turns for context window
    ];

    // Remove thinking, show AI bubble with streaming
    thinkingEl.remove();
    const { bubble, content: contentEl } = appendAiBubble(snippets);

    try {
      await streamLLM(finalMessages, token => {
        contentEl.innerHTML = renderMarkdown(contentEl.dataset.raw + token);
        contentEl.dataset.raw = (contentEl.dataset.raw || '') + token;
        messages.scrollTop = messages.scrollHeight;
      });

      // Remove streaming cursor after done
      const cursor = contentEl.querySelector('.cursor');
      if (cursor) cursor.remove();

      const finalContent = contentEl.dataset.raw || '';
      if (!finalContent) {
        contentEl.innerHTML = '<em style="color:var(--text-3)">Model returned an empty response. Try a different model.</em>';
      }
      state.history.push({ role: 'assistant', content: finalContent });

    } catch (streamErr) {
      // Show error inside the already-visible bubble instead of a duplicate
      contentEl.innerHTML = `<span style="color:#ef4444">❌ ${escapeHtml(streamErr.message)}</span>`;
      showToast(streamErr.message, 'error', 6000);
      console.error('[streamLLM]', streamErr);
    }

  } catch (e) {
    thinkingEl.remove();
    appendMsg('ai', `❌ Error: ${e.message}`);
    showToast(e.message, 'error');
  } finally {
    state.streaming = false;
    updateSendBtn();
  }
}

// ─── LLM helpers ──────────────────────────────────────────────────────────────
async function completeLLM(messages) {
  const body = { apiKey: state.apiKey, model: state.model, messages };
  const r = await fetch('/api/chat', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);

  const reader  = r.body.getReader();
  const decoder = new TextDecoder();
  let full = '';
  let buf  = '';

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const line of lines) {
      const dat = line.replace(/^data:\s*/, '').trim();
      if (!dat || dat === '[DONE]') continue;
      let j;
      try { j = JSON.parse(dat); } catch { continue; }  // skip malformed SSE lines
      if (j.error) throw new Error(j.error.message || JSON.stringify(j.error));
      const tok = j.choices?.[0]?.delta?.content || '';
      full += tok;
    }
  }
  return full;
}

async function streamLLM(messages, onToken) {
  const body = { apiKey: state.apiKey, model: state.model, messages };
  const r = await fetch('/api/chat', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!r.ok) {
    const txt = await r.text();
    throw new Error(`LLM error (${r.status}): ${txt}`);
  }

  const reader  = r.body.getReader();
  const decoder = new TextDecoder();
  let buf = '';

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const line of lines) {
      const dat = line.replace(/^data:\s*/, '').trim();
      if (!dat || dat === '[DONE]') continue;
      let j;
      try { j = JSON.parse(dat); } catch { continue; }  // skip malformed SSE lines
      // Throw real API errors so they surface to the user
      if (j.error) throw new Error(j.error.message || JSON.stringify(j.error));
      const tok = j.choices?.[0]?.delta?.content || '';
      if (tok) onToken(tok);
    }
  }
}

// ─── DOM helpers ──────────────────────────────────────────────────────────────
function appendMsg(role, content) {
  const isUser = role === 'user';
  const div = document.createElement('div');
  div.className = `msg ${role}`;
  const bubble = renderMarkdown(content);
  div.innerHTML = `
    <div class="msg-avatar">${isUser ? '👤' : '⚡'}</div>
    <div class="msg-content">
      <div class="msg-bubble">${bubble}</div>
      <div class="msg-meta">${new Date().toLocaleTimeString()}</div>
    </div>`;
  messages.appendChild(div);
  messages.scrollTop = messages.scrollHeight;
  return div;
}

function appendThinking() {
  const div = document.createElement('div');
  div.className = 'msg ai';
  div.innerHTML = `
    <div class="msg-avatar">⚡</div>
    <div class="msg-content">
      <div class="msg-bubble">
        <div class="thinking-dots"><span></span><span></span><span></span></div>
      </div>
    </div>`;
  messages.appendChild(div);
  messages.scrollTop = messages.scrollHeight;
  return div;
}

function appendAiBubble(snippets) {
  const div = document.createElement('div');
  div.className = 'msg ai';

  const snippetBtn = snippets.length
    ? `<button class="msg-retrieval-btn" onclick="showSnippets(this)">⚡ ${snippets.length} retrieved</button>`
    : '';

  div.innerHTML = `
    <div class="msg-avatar">⚡</div>
    <div class="msg-content">
      <div class="msg-bubble" data-raw=""><span class="cursor"></span></div>
      <div class="msg-meta">${new Date().toLocaleTimeString()} ${snippetBtn}</div>
    </div>`;

  // Store snippets on the button
  if (snippets.length) {
    const btn = div.querySelector('.msg-retrieval-btn');
    btn._snippets = snippets;
  }

  messages.appendChild(div);
  messages.scrollTop = messages.scrollHeight;

  return { bubble: div, content: div.querySelector('.msg-bubble') };
}

window.showSnippets = function(btn) {
  const snips = btn._snippets || [];
  state.lastSnippets = snips;
  renderRetrievalDrawer(snips);
  retrievalDrawer.hidden = false;
};

// ─── Lightweight Markdown renderer ────────────────────────────────────────────
function renderMarkdown(text) {
  if (!text) return '<span class="cursor"></span>';
  let html = escapeHtml(text);

  // Code blocks (``` ... ```)
  html = html.replace(/```(\w*)\n?([\s\S]*?)```/g, (_, lang, code) =>
    `<pre><code class="lang-${lang}">${code}</code></pre>`);
  // Inline code
  html = html.replace(/`([^`]+)`/g, '<code>$1</code>');
  // Bold
  html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  // Italic
  html = html.replace(/\*(.+?)\*/g, '<em>$1</em>');
  // Headings
  html = html.replace(/^### (.+)$/gm, '<h3>$1</h3>');
  html = html.replace(/^## (.+)$/gm,  '<h2>$1</h2>');
  html = html.replace(/^# (.+)$/gm,   '<h1>$1</h1>');
  // Blockquote
  html = html.replace(/^&gt; (.+)$/gm, '<blockquote>$1</blockquote>');
  // Unordered list
  html = html.replace(/^\* (.+)$/gm,  '<li>$1</li>');
  html = html.replace(/^- (.+)$/gm,   '<li>$1</li>');
  // Ordered list
  html = html.replace(/^\d+\. (.+)$/gm, '<li>$1</li>');
  // Wrap consecutive <li> in <ul>
  html = html.replace(/(<li>[\s\S]+?<\/li>)(?=\s*(?:<li>|$))/g, m => `<ul>${m}</ul>`);
  // Paragraphs from double newlines
  html = html.replace(/\n\n/g, '</p><p>');
  html = html.replace(/\n/g, '<br>');
  if (!html.startsWith('<')) html = `<p>${html}</p>`;

  // Add streaming cursor at end if no pre/h elements
  html += '<span class="cursor"></span>';
  return html;
}

function escapeHtml(text) {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// ─── Initial send button state ────────────────────────────────────────────────
updateSendBtn();

// ─── Boot: fetch models from OpenRouter ──────────────────────────────────────
fetchModels();
