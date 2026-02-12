export const config = { runtime: 'edge' };

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
    const upstream = await fetch('https://chatgpt.com/backend-api/wham/usage', {
      headers: {
        'Authorization': `Bearer ${token}`,
        'Accept': 'application/json',
      },
    });

    const body = await upstream.text();
    const contentType = upstream.headers.get('content-type') || 'application/json';

    // Upstream 403 often returns an HTML challenge page (Cloudflare/OpenAI edge).
    if (!upstream.ok && upstream.status === 403 && contentType.includes('text/html')) {
      return json(403, {
        error: 'Upstream blocked request (OpenAI edge / Cloudflare)',
        detail: 'Vercel deployment IP was blocked by chatgpt.com. Run locally or use a non-blocked egress.',
        upstream_status: upstream.status,
      }, req);
    }

    // Forward all rate-limit-related headers for debugging
    const respHeaders = { ...corsHeaders(req), 'Content-Type': contentType };
    for (const [k, v] of upstream.headers.entries()) {
      if (k.startsWith('x-ratelimit') || k.startsWith('x-rate-limit') || k === 'retry-after') {
        respHeaders[`x-upstream-${k}`] = v;
      }
    }

    return new Response(body, { status: upstream.status, headers: respHeaders });
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
