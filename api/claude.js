export const config = { runtime: 'edge' };
const ANTHROPIC_OAUTH_BETA = 'oauth-2025-04-20';

export default async function handler(req) {
  if (req.method === 'OPTIONS') {
    return new Response(null, { status: 204, headers: corsHeaders(req) });
  }
  if (req.method !== 'GET') {
    return json(405, { error: 'Method not allowed' }, req);
  }

  const token = req.headers.get('x-quota-token');
  if (!token) {
    return json(400, { error: 'x-quota-token header required' }, req);
  }

  try {
    const upstream = await fetch('https://api.anthropic.com/api/oauth/usage', {
      headers: {
        'Authorization': `Bearer ${token}`,
        'Accept': 'application/json',
        'anthropic-beta': ANTHROPIC_OAUTH_BETA,
      },
    });

    const body = await upstream.text();
    return new Response(body, {
      status: upstream.status,
      headers: {
        ...corsHeaders(req),
        'Content-Type': upstream.headers.get('Content-Type') || 'application/json',
      },
    });
  } catch (e) {
    return json(502, { error: 'Upstream request failed', detail: e.message }, req);
  }
}

function corsHeaders(req) {
  return {
    'Access-Control-Allow-Origin': req.headers.get('Origin') || '*',
    'Access-Control-Allow-Methods': 'GET, OPTIONS',
    'Access-Control-Allow-Headers': 'x-quota-token',
    'Access-Control-Max-Age': '86400',
  };
}

function json(status, data, req) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { ...corsHeaders(req), 'Content-Type': 'application/json' },
  });
}
