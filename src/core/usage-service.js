const { fetchClaudeUsageRaw, fetchCodexUsageRaw, safeJsonParse } = require('./usage-clients');
const { parseClaudeUsage, parseCodexUsage } = require('./parsers');

function buildError(raw, parsed) {
  if (parsed && typeof parsed === 'object') {
    return parsed.error || parsed.detail || `HTTP ${raw.status}`;
  }
  if (raw.status === 403 && raw.contentType.includes('text/html')) {
    return 'Upstream blocked request (OpenAI edge / Cloudflare)';
  }
  return `HTTP ${raw.status}`;
}

async function fetchNormalizedUsage(service, token) {
  let raw;
  if (service === 'claude') {
    raw = await fetchClaudeUsageRaw(token);
  } else if (service === 'codex') {
    raw = await fetchCodexUsageRaw(token);
  } else {
    throw new Error('Unsupported service');
  }

  const parsed = safeJsonParse(raw.body);
  if (!raw.ok) {
    throw new Error(buildError(raw, parsed));
  }
  if (!parsed) {
    throw new Error('Upstream returned non-JSON response');
  }

  const windows = service === 'claude' ? parseClaudeUsage(parsed) : parseCodexUsage(parsed);

  return {
    raw: parsed,
    windows,
  };
}

module.exports = {
  fetchNormalizedUsage,
};
