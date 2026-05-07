//! Static HTML dashboard served at `GET /dashboard`.
//!
//! Single-file vanilla JS — fetches `/dashboard/metrics` every 5s
//! and renders the headline tiles. No build step, no dependency on
//! Node tooling. Suitable for ops-style monitoring; the rich UI
//! lives in the macOS desktop client.

pub const DASHBOARD_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Jarvis Dashboard</title>
<style>
  :root { color-scheme: dark light; }
  body { font: 14px/1.5 system-ui, -apple-system, sans-serif;
         margin: 0; padding: 24px;
         background: #0e0f12; color: #e7e9ee; }
  h1 { margin: 0 0 16px; font-size: 18px; font-weight: 600; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
          gap: 12px; max-width: 960px; }
  .tile { background: #16181d; border: 1px solid #2a2d34; border-radius: 8px;
          padding: 16px; }
  .tile .label { font-size: 12px; color: #8a8f99; text-transform: uppercase;
                 letter-spacing: 0.04em; }
  .tile .value { font-size: 28px; font-weight: 600; margin-top: 6px;
                 font-variant-numeric: tabular-nums; }
  .footer { margin-top: 24px; color: #6b6f78; font-size: 12px; }
  .pill { display: inline-block; padding: 2px 10px; border-radius: 999px;
          background: #1f2229; color: #b9bdc6; font-size: 12px; }
</style>
</head>
<body>
  <h1>Jarvis Dashboard <span class="pill" id="ts">…</span></h1>
  <div class="grid">
    <div class="tile"><div class="label">Active Sessions</div>
                       <div class="value" data-key="active_sessions">…</div></div>
    <div class="tile"><div class="label">Raw Events</div>
                       <div class="value" data-key="raw_event_count">…</div></div>
    <div class="tile"><div class="label">Memories</div>
                       <div class="value" data-key="memory_count">…</div></div>
    <div class="tile"><div class="label">Outbox Pending</div>
                       <div class="value" data-key="outbox_pending">…</div></div>
    <div class="tile"><div class="label">Route Decisions</div>
                       <div class="value" data-key="route_decisions">…</div></div>
    <div class="tile"><div class="label">Promoted Artifacts</div>
                       <div class="value" data-key="promoted_artifacts">…</div></div>
  </div>
  <div class="footer">
    Auto-refresh every 5s · <a href="/healthz" style="color:#6b6f78">/healthz</a>
  </div>
<script>
async function refresh() {
  try {
    const r = await fetch("/dashboard/metrics", { cache: "no-store" });
    if (!r.ok) return;
    const data = await r.json();
    for (const el of document.querySelectorAll("[data-key]")) {
      const v = data[el.dataset.key];
      if (v !== undefined) el.textContent = v;
    }
    document.getElementById("ts").textContent = data.ts || "";
  } catch (_) { /* keep last values */ }
}
refresh();
setInterval(refresh, 5000);
</script>
</body>
</html>"##;
