"use client";

import { useCallback, useEffect, useState } from "react";
import { api, errorDetail } from "@/lib/api";

type Settings = {
  environment: string;
  edition: string;
  operator: {
    default_currency: string;
    qr_ttl_seconds: number;
    webhook_timeout_ms: number;
    rate_limit_per_minute: number;
    cors_allow_origins: string[];
    webhook_url_allowlist: string[];
    features: {
      connector_mock: boolean;
      connector_open_banking: boolean;
      telemetry_opt_in: boolean;
    };
  };
  operator_source: Record<string, string>;
  env_only: Array<{ key: string; configured: boolean; hint: string }>;
  process_notes: Record<string, string>;
  sandbox_lab?: { available: boolean; message?: string };
};

type ApiKey = {
  id: string;
  name: string;
  fingerprint: string;
  revoked: boolean;
  scopes: string[];
  merchant_id?: string;
};

type Webhook = {
  id: string;
  url: string;
  status: string;
  failure_count: number;
  event_types: string[];
  signing_secret_kind?: string;
};

type Policy = {
  id: string;
  name: string;
  status: string;
  rules_json: unknown;
  fallback_policy: unknown;
};

type Connector = {
  key: string;
  name: string;
  status: string;
  health?: unknown;
  health_status?: string;
  registered: boolean;
  sandbox_only?: boolean;
  configuration_kind: string;
  configuration_ref: string;
  connector_type: string;
};

type Props = { token: string };

export default function ConfigDesk({ token }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [hooks, setHooks] = useState<Webhook[]>([]);
  const [policies, setPolicies] = useState<Policy[]>([]);
  const [connectors, setConnectors] = useState<Connector[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [keyName, setKeyName] = useState("Chiave POS");
  const [freshSecret, setFreshSecret] = useState<string | null>(null);
  const [hookUrl, setHookUrl] = useState("");
  const [hookEvents, setHookEvents] = useState(
    "payment.created, payment.settled, payment.failed",
  );
  const [freshHookSecret, setFreshHookSecret] = useState<string | null>(null);
  const [currency, setCurrency] = useState("EUR");
  const [qrTtl, setQrTtl] = useState("300");
  const [webhookTimeout, setWebhookTimeout] = useState("5000");
  const [rateLimit, setRateLimit] = useState("120");
  const [cors, setCors] = useState("");
  const [allowlist, setAllowlist] = useState("");
  const [mock, setMock] = useState(true);
  const [openBanking, setOpenBanking] = useState(false);
  const [telemetry, setTelemetry] = useState(false);
  const [policyEdits, setPolicyEdits] = useState<Record<string, { name: string; rules: string; fallback: string; status: string }>>(
    {},
  );
  const [connectorRefs, setConnectorRefs] = useState<Record<string, string>>({});

  const load = useCallback(async () => {
    setError(null);
    const [s, k, w, r, c] = await Promise.all([
      api<Settings>("/v1/admin/settings", token),
      api<ApiKey[]>("/v1/admin/api-keys", token),
      api<Webhook[]>("/v1/admin/webhook-endpoints", token),
      api<Policy[]>("/v1/admin/routing-policies", token),
      api<{ connectors: Connector[] }>("/v1/admin/connectors", token),
    ]);
    if (!s.ok) {
      setError(errorDetail(s.body, "Impostazioni non disponibili"));
      return;
    }
    setSettings(s.body);
    const op = s.body.operator;
    setCurrency(op.default_currency);
    setQrTtl(String(op.qr_ttl_seconds));
    setWebhookTimeout(String(op.webhook_timeout_ms));
    setRateLimit(String(op.rate_limit_per_minute));
    setCors(op.cors_allow_origins.join("\n"));
    setAllowlist(op.webhook_url_allowlist.join("\n"));
    setMock(op.features.connector_mock);
    setOpenBanking(op.features.connector_open_banking);
    setTelemetry(op.features.telemetry_opt_in);
    if (k.ok && Array.isArray(k.body)) setKeys(k.body);
    if (w.ok && Array.isArray(w.body)) setHooks(w.body);
    if (r.ok && Array.isArray(r.body)) {
      setPolicies(r.body);
      const edits: Record<string, { name: string; rules: string; fallback: string; status: string }> = {};
      for (const p of r.body) {
        edits[p.id] = {
          name: p.name,
          rules: JSON.stringify(p.rules_json, null, 2),
          fallback: JSON.stringify(p.fallback_policy, null, 2),
          status: p.status,
        };
      }
      setPolicyEdits(edits);
    }
    if (c.ok && Array.isArray(c.body.connectors)) {
      setConnectors(c.body.connectors);
      const refs: Record<string, string> = {};
      for (const row of c.body.connectors) {
        refs[row.key] = row.configuration_ref.startsWith("********") ? "" : row.configuration_ref;
      }
      setConnectorRefs(refs);
    }
  }, [token]);

  useEffect(() => {
    void load();
  }, [load]);

  async function saveOperator() {
    setError(null);
    setNotice(null);
    const res = await api<Settings>("/v1/admin/settings", token, {
      method: "PATCH",
      body: JSON.stringify({
        default_currency: currency,
        qr_ttl_seconds: Number(qrTtl),
        webhook_timeout_ms: Number(webhookTimeout),
        rate_limit_per_minute: Number(rateLimit),
        cors_allow_origins: lines(cors),
        webhook_url_allowlist: lines(allowlist),
        features: {
          connector_mock: mock,
          connector_open_banking: openBanking,
          telemetry_opt_in: telemetry,
        },
      }),
    });
    if (!res.ok) {
      setError(errorDetail(res.body, "Salvataggio impostazioni non riuscito"));
      return;
    }
    setSettings(res.body);
    setNotice("Impostazioni operative salvate nel tenant.");
  }

  async function createKey() {
    setError(null);
    setFreshSecret(null);
    const res = await api<{ secret?: string; name: string }>("/v1/admin/api-keys", token, {
      method: "POST",
      body: JSON.stringify({ name: keyName }),
    });
    if (!res.ok) {
      setError(errorDetail(res.body, "Creazione chiave non riuscita"));
      return;
    }
    setFreshSecret(res.body.secret ?? null);
    await load();
  }

  async function revokeKey(id: string) {
    setError(null);
    const res = await api(`/v1/admin/api-keys/${id}/revoke`, token, { method: "POST" });
    if (!res.ok) setError(errorDetail(res.body, "Revoca non riuscita"));
    await load();
  }

  async function createHook() {
    setError(null);
    setFreshHookSecret(null);
    const events = hookEvents
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const res = await api<{ secret?: string }>("/v1/admin/webhook-endpoints", token, {
      method: "POST",
      body: JSON.stringify({ url: hookUrl, event_types: events }),
    });
    if (!res.ok) {
      setError(errorDetail(res.body, "Creazione webhook non riuscita"));
      return;
    }
    setFreshHookSecret(res.body.secret ?? null);
    setHookUrl("");
    await load();
  }

  async function patchHook(id: string, body: Record<string, unknown>) {
    setError(null);
    const res = await api(`/v1/admin/webhook-endpoints/${id}`, token, {
      method: "PATCH",
      body: JSON.stringify(body),
    });
    if (!res.ok) setError(errorDetail(res.body, "Aggiornamento webhook non riuscito"));
    await load();
  }

  async function rotateHook(id: string) {
    setError(null);
    const res = await api<{ secret?: string }>(`/v1/admin/webhook-endpoints/${id}/rotate-secret`, token, {
      method: "POST",
    });
    if (!res.ok) {
      setError(errorDetail(res.body, "Rotazione secret non riuscita"));
      return;
    }
    setFreshHookSecret(res.body.secret ?? null);
  }

  async function savePolicy(id: string) {
    const edit = policyEdits[id];
    if (!edit) return;
    setError(null);
    let rules: unknown;
    let fallback: unknown;
    try {
      rules = JSON.parse(edit.rules);
      fallback = JSON.parse(edit.fallback);
    } catch {
      setError("JSON policy non valido");
      return;
    }
    const res = await api(`/v1/admin/routing-policies/${id}`, token, {
      method: "PATCH",
      body: JSON.stringify({
        name: edit.name,
        rules_json: rules,
        fallback_policy: fallback,
        status: edit.status,
      }),
    });
    if (!res.ok) setError(errorDetail(res.body, "Salvataggio routing non riuscito"));
    else setNotice("Policy di routing aggiornata (si usa la policy active più recente).");
    await load();
  }

  async function saveConnector(key: string, status: string) {
    setError(null);
    const ref = connectorRefs[key]?.trim();
    const res = await api(`/v1/admin/connectors/${key}`, token, {
      method: "PATCH",
      body: JSON.stringify({
        status,
        configuration_ref: ref ? ref : undefined,
      }),
    });
    if (!res.ok) setError(errorDetail(res.body, "Aggiornamento connettore non riuscito"));
    else setNotice("Connettore aggiornato. Non è una configurazione Stripe live.");
    await load();
  }

  return (
    <div className="space-y-6">
      {error && <p className="text-sm text-signal">{error}</p>}
      {notice && <p className="text-sm text-rail">{notice}</p>}

      <section className="border border-ink/10 bg-white">
        <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">
          Impostazioni operative
        </h2>
        <div className="grid gap-4 p-4 text-sm md:grid-cols-2">
          <label>
            Valuta di default
            <input className="mt-1 w-full border border-ink/20 px-2 py-1 font-mono uppercase" maxLength={3} value={currency} onChange={(e) => setCurrency(e.target.value)} />
          </label>
          <label>
            TTL QR (secondi)
            <input className="mt-1 w-full border border-ink/20 px-2 py-1 font-mono" value={qrTtl} onChange={(e) => setQrTtl(e.target.value)} />
          </label>
          <label>
            Timeout webhook (ms)
            <input className="mt-1 w-full border border-ink/20 px-2 py-1 font-mono" value={webhookTimeout} onChange={(e) => setWebhookTimeout(e.target.value)} />
          </label>
          <label>
            Rate limit / minuto
            <input className="mt-1 w-full border border-ink/20 px-2 py-1 font-mono" value={rateLimit} onChange={(e) => setRateLimit(e.target.value)} />
          </label>
          <label className="md:col-span-2">
            Origini CORS (una per riga)
            <textarea className="mt-1 h-20 w-full border border-ink/20 px-2 py-1 font-mono text-xs" value={cors} onChange={(e) => setCors(e.target.value)} />
          </label>
          <label className="md:col-span-2">
            Allowlist hostname webhook (una per riga)
            <textarea className="mt-1 h-20 w-full border border-ink/20 px-2 py-1 font-mono text-xs" value={allowlist} onChange={(e) => setAllowlist(e.target.value)} />
          </label>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={mock} onChange={(e) => setMock(e.target.checked)} />
            Connettore mock (sandbox)
          </label>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={openBanking} onChange={(e) => setOpenBanking(e.target.checked)} />
            Stub open banking
          </label>
          <label className="flex items-center gap-2 md:col-span-2">
            <input type="checkbox" checked={telemetry} onChange={(e) => setTelemetry(e.target.checked)} />
            Telemetria opt-in
          </label>
          <button type="button" className="bg-ink px-3 py-2 text-ticket" onClick={() => void saveOperator()}>
            Salva impostazioni
          </button>
          <p className="text-xs text-ledger md:col-span-2">
            Valuta, TTL, CORS, rate limit, timeout e allowlist restano nel tenant Postgres e si applicano
            alle nuove richieste. Mock e stub si registrano all&apos;avvio: se il processo è partito senza
            FEATURE_CONNECTOR_MOCK, serve riavvio dopo averlo acceso nel .env. La telemetria del processo
            corrente resta quella di avvio.
          </p>
        </div>
      </section>

      <section className="border border-ink/10 bg-white">
        <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Chiavi API</h2>
        <div className="space-y-3 p-4 text-sm">
          <div className="flex flex-wrap gap-2">
            <input className="border border-ink/20 px-2 py-1" value={keyName} onChange={(e) => setKeyName(e.target.value)} />
            <button type="button" className="border border-ink px-3 py-1" onClick={() => void createKey()}>
              Crea chiave
            </button>
          </div>
          {freshSecret && (
            <p className="border border-signal/30 bg-paper p-2 font-mono text-xs">
              Secret (una sola volta): {freshSecret}
            </p>
          )}
          <ul className="space-y-2">
            {keys.map((k) => (
              <li key={k.id} className="flex flex-wrap items-center justify-between gap-2 border-b border-ink/5 py-2">
                <span className="font-mono text-xs">
                  {k.name} · {k.id} · fp {k.fingerprint.slice(0, 12)}… {k.revoked ? "· revocata" : ""}
                </span>
                {!k.revoked && (
                  <button type="button" className="underline" onClick={() => void revokeKey(k.id)}>
                    Revoca
                  </button>
                )}
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section className="border border-ink/10 bg-white">
        <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Webhook</h2>
        <div className="space-y-3 p-4 text-sm">
          <label className="block">
            URL
            <input className="mt-1 w-full border border-ink/20 px-2 py-1 font-mono text-xs" value={hookUrl} onChange={(e) => setHookUrl(e.target.value)} placeholder="http://demo-merchant:3002/webhooks/openpay" />
          </label>
          <label className="block">
            Eventi (separati da virgola)
            <input className="mt-1 w-full border border-ink/20 px-2 py-1 font-mono text-xs" value={hookEvents} onChange={(e) => setHookEvents(e.target.value)} />
          </label>
          <button type="button" className="border border-ink px-3 py-1" onClick={() => void createHook()}>
            Crea endpoint
          </button>
          {freshHookSecret && (
            <p className="border border-signal/30 bg-paper p-2 font-mono text-xs">
              Secret endpoint (una sola volta): {freshHookSecret}
            </p>
          )}
          <ul className="space-y-3">
            {hooks.map((h) => (
              <li key={h.id} className="border border-ink/10 p-3">
                <p className="font-mono text-xs">{h.id} · {h.url} · {h.status} · fail {h.failure_count}</p>
                <p className="text-xs text-ledger">firma: {h.signing_secret_kind === "env" ? "WEBHOOK_SIGNING_SECRET (.env)" : "secret dell’endpoint"}</p>
                <div className="mt-2 flex flex-wrap gap-2">
                  {h.status === "active" ? (
                    <button type="button" className="underline" onClick={() => void patchHook(h.id, { status: "disabled" })}>
                      Disabilita
                    </button>
                  ) : (
                    <button type="button" className="underline" onClick={() => void patchHook(h.id, { status: "active" })}>
                      Attiva
                    </button>
                  )}
                  <button type="button" className="underline" onClick={() => void rotateHook(h.id)}>
                    Ruota secret
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section className="border border-ink/10 bg-white">
        <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Routing</h2>
        <div className="space-y-4 p-4 text-sm">
          {policies.map((p) => {
            const edit = policyEdits[p.id];
            if (!edit) return null;
            return (
              <div key={p.id} className="border border-ink/10 p-3">
                <label className="block">
                  Nome
                  <input className="mt-1 w-full border border-ink/20 px-2 py-1" value={edit.name} onChange={(e) => setPolicyEdits((prev) => ({ ...prev, [p.id]: { ...edit, name: e.target.value } }))} />
                </label>
                <label className="mt-2 block">
                  rules_json
                  <textarea className="mt-1 h-32 w-full border border-ink/20 px-2 py-1 font-mono text-xs" value={edit.rules} onChange={(e) => setPolicyEdits((prev) => ({ ...prev, [p.id]: { ...edit, rules: e.target.value } }))} />
                </label>
                <label className="mt-2 block">
                  fallback
                  <textarea className="mt-1 h-24 w-full border border-ink/20 px-2 py-1 font-mono text-xs" value={edit.fallback} onChange={(e) => setPolicyEdits((prev) => ({ ...prev, [p.id]: { ...edit, fallback: e.target.value } }))} />
                </label>
                <label className="mt-2 flex items-center gap-2">
                  <input type="checkbox" checked={edit.status === "active"} onChange={(e) => setPolicyEdits((prev) => ({ ...prev, [p.id]: { ...edit, status: e.target.checked ? "active" : "disabled" } }))} />
                  Attiva
                </label>
                <button type="button" className="mt-2 border border-ink px-3 py-1" onClick={() => void savePolicy(p.id)}>
                  Salva policy
                </button>
              </div>
            );
          })}
        </div>
      </section>

      <section className="border border-ink/10 bg-white">
        <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Connettori</h2>
        <div className="space-y-3 p-4 text-sm">
          <p className="text-ledger">
            Abilita/disabilita i binari sandbox e aggiorna un riferimento <span className="font-mono">secret://</span> o{" "}
            <span className="font-mono">env:</span> (cifrato a rest). Non configura Stripe o altri PSP live.
          </p>
          {connectors.map((c) => (
            <div key={c.key} className="border border-ink/10 p-3">
              <p className="font-mono text-xs">
                {c.key} · {c.name} · {c.status} · health {String(c.health ?? c.health_status ?? "n/d")} ·{" "}
                {c.registered ? "in processo" : "non registrato (riavvio?)"}
              </p>
              <label className="mt-2 block text-xs">
                configuration_ref
                <input
                  className="mt-1 w-full border border-ink/20 px-2 py-1 font-mono"
                  placeholder={c.configuration_ref}
                  value={connectorRefs[c.key] ?? ""}
                  onChange={(e) => setConnectorRefs((prev) => ({ ...prev, [c.key]: e.target.value }))}
                />
              </label>
              <div className="mt-2 flex gap-2">
                <button type="button" className="underline" onClick={() => void saveConnector(c.key, c.status === "enabled" ? "disabled" : "enabled")}>
                  {c.status === "enabled" ? "Disabilita" : "Abilita"}
                </button>
                <button type="button" className="underline" onClick={() => void saveConnector(c.key, c.status)}>
                  Salva ref
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="border border-ink/10 bg-white">
        <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Solo ambiente / riavvio</h2>
        <div className="space-y-2 p-4 text-sm">
          <p className="text-ledger">
            Questi restano nel <span className="font-mono">.env</span> perché un form web è il modo più semplice
            per perderli. Non vengono mostrati in chiaro.
          </p>
          <ul className="space-y-2">
            {(settings?.env_only ?? []).map((row) => (
              <li key={row.key}>
                <strong className="font-mono text-xs">{row.key}</strong>{" "}
                <span className="text-xs uppercase text-rail">{row.configured ? "impostato" : "mancante"}</span>
                <p className="text-xs text-ledger">{row.hint}</p>
              </li>
            ))}
          </ul>
          {settings?.process_notes && (
            <ul className="mt-4 list-disc space-y-1 pl-4 text-xs text-ledger">
              {Object.values(settings.process_notes).map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          )}
        </div>
      </section>
    </div>
  );
}

function lines(raw: string): string[] {
  return raw
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}
