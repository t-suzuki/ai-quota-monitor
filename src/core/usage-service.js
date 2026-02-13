const { fetchClaudeUsageRaw, fetchCodexUsageRaw, safeJsonParse } = require('./usage-clients');
const { parseClaudeUsage, parseCodexUsage } = require('./parsers');

function buildError(raw, parsed) {
  if (raw.status === 403 && raw.contentType.includes('text/html')) {
    return 'Upstream blocked request (OpenAI edge / Cloudflare)';
  }
  if (raw.status === 400) return 'Upstream rejected request (HTTP 400)';
  if (raw.status === 401) return 'Authentication failed (HTTP 401)';
  if (raw.status === 403) return 'Permission denied by upstream (HTTP 403)';
  if (raw.status === 404) return 'Upstream endpoint not found (HTTP 404)';
  if (raw.status === 429) return 'Upstream rate limit exceeded (HTTP 429)';
  if (raw.status >= 500 && raw.status < 600) return `Upstream server error (HTTP ${raw.status})`;
  return `Upstream request failed (HTTP ${raw.status})`;
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
