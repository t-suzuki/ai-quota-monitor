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

function createUsageService(deps = {}) {
  const fetchClaude = deps.fetchClaudeUsageRaw || fetchClaudeUsageRaw;
  const fetchCodex = deps.fetchCodexUsageRaw || fetchCodexUsageRaw;
  const parseJson = deps.safeJsonParse || safeJsonParse;
  const parseClaude = deps.parseClaudeUsage || parseClaudeUsage;
  const parseCodex = deps.parseCodexUsage || parseCodexUsage;

  async function fetchNormalizedUsage(service, token) {
    let raw;
    if (service === 'claude') {
      raw = await fetchClaude(token);
    } else if (service === 'codex') {
      raw = await fetchCodex(token);
    } else {
      throw new Error('Unsupported service');
    }

    const parsed = parseJson(raw.body);
    if (!raw.ok) {
      throw new Error(buildError(raw, parsed));
    }
    if (!parsed) {
      throw new Error('Upstream returned non-JSON response');
    }

    const windows = service === 'claude' ? parseClaude(parsed) : parseCodex(parsed);
    return { raw: parsed, windows };
  }

  return { fetchNormalizedUsage };
}

const { fetchNormalizedUsage } = createUsageService();

module.exports = {
  buildError,
  createUsageService,
  fetchNormalizedUsage,
};
