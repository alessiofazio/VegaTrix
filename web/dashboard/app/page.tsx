"use client";

import { useCallback, useEffect, useState } from "react";

const API = process.env.NEXT_PUBLIC_API_BASE_URL ?? "http://localhost:8080";

const STATIONS = ["PENDING", "PROCESSING", "SETTLED", "FAILED", "REQUIRES_ACTION"];

type Overview = {
  edition: string;
  self_hosted: boolean;
  payment_counts: Record<string, number>;
  payments: Array<{
    id: string;
    status: string;
    amount_minor: number;
    currency: string;
    merchant_order_id: string;
    created_at: string;
  }>;
};

type Detail = {
  pay: Record<string, unknown>;
  events: unknown[];
  attempts: Array<{ id: string; status: string; connector_key: string; provider_reference?: string }>;
  deliveries: unknown[];
};

export default function Page() {
  const [email, setEmail] = useState("admin@demo.openpay.local");
  const [password, setPassword] = useState("ChangeMeNow_OpenPayDemo1");
  const [token, setToken] = useState<string | null>(null);
  const [data, setData] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<Detail | null>(null);

  const authHeaders = useCallback(
    () => ({ Authorization: `Bearer ${token}`, "content-type": "application/json" }),
    [token],
  );

  async function login(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const res = await fetch(`${API}/v1/auth/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ email, password }),
    });
    const body = await res.json();
    if (!res.ok) {
      setError(body.detail ?? "Login failed");
      return;
    }
    setToken(body.access_token);
  }

  const refreshOverview = useCallback(async () => {
    if (!token) return;
    const res = await fetch(`${API}/v1/admin/overview`, { headers: authHeaders() });
    setData(await res.json());
  }, [token, authHeaders]);

  useEffect(() => {
    void refreshOverview();
    const t = setInterval(() => void refreshOverview(), 5000);
    return () => clearInterval(t);
  }, [refreshOverview]);

  async function openPayment(id: string) {
    if (!token) return;
    setSelected(id);
    const [pay, events, attempts, deliveries] = await Promise.all([
      fetch(`${API}/v1/payment-requests/${id}`, { headers: authHeaders() }).then((r) => r.json()),
      fetch(`${API}/v1/payment-requests/${id}/events`, { headers: authHeaders() }).then((r) => r.json()),
      fetch(`${API}/v1/payment-requests/${id}/attempts`, { headers: authHeaders() }).then((r) => r.json()),
      fetch(`${API}/v1/admin/webhook-deliveries`, { headers: authHeaders() }).then((r) => r.json()),
    ]);
    setDetail({ pay, events, attempts, deliveries });
  }

  async function reconcile(id: string) {
    if (!token) return;
    await fetch(`${API}/v1/admin/payments/${id}/reconcile`, { method: "POST", headers: authHeaders() });
    await openPayment(id);
    await refreshOverview();
  }

  async function resolveAttempt(attemptId: string, approve: boolean) {
    if (!token) return;
    await fetch(`${API}/v1/admin/attempts/${attemptId}/resolve`, {
      method: "POST",
      headers: authHeaders(),
      body: JSON.stringify({ approve }),
    });
    if (selected) await openPayment(selected);
    await refreshOverview();
  }

  if (!token) {
    return (
      <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center px-6">
        <p className="text-xs uppercase tracking-[0.2em] text-rail">OpenPay Protocol · sandbox</p>
        <h1 className="mt-3 text-4xl font-semibold">Clearing desk</h1>
        <p className="mt-2 text-ledger">Sign in to inspect payment rails, attempts, and audit stations.</p>
        <form onSubmit={login} className="mt-8 space-y-4 rounded-sm border border-ink/10 bg-white p-6">
          <label className="block text-sm">
            Email
            <input className="mt-1 w-full border border-ink/20 px-3 py-2" value={email} onChange={(e) => setEmail(e.target.value)} />
          </label>
          <label className="block text-sm">
            Password
            <input type="password" className="mt-1 w-full border border-ink/20 px-3 py-2" value={password} onChange={(e) => setPassword(e.target.value)} />
          </label>
          {error && <p className="text-sm text-signal">{error}</p>}
          <button type="submit" className="w-full bg-ink px-4 py-2 text-ticket">Enter desk</button>
        </form>
      </main>
    );
  }

  return (
    <main className="mx-auto max-w-6xl px-6 py-8">
      <header className="flex items-end justify-between gap-6">
        <div>
          <p className="text-xs uppercase tracking-[0.2em] text-rail">
            {data?.self_hosted ? "self-hosted" : "cloud"} · {data?.edition}
          </p>
          <h1 className="text-3xl font-semibold">Payment rails</h1>
        </div>
        <button type="button" className="text-sm underline" onClick={() => setToken(null)}>Sign out</button>
      </header>
      <div className="rail mt-4" />
      <section className="mt-6 grid grid-cols-2 gap-4 md:grid-cols-5">
        {STATIONS.map((s) => (
          <article key={s} className="border border-ink/10 bg-white p-4">
            <p className="text-xs uppercase tracking-widest text-ledger">{s}</p>
            <p className="font-mono text-3xl">{data?.payment_counts?.[s] ?? 0}</p>
          </article>
        ))}
      </section>
      <section className="mt-8 grid gap-6 md:grid-cols-2">
        <div className="border border-ink/10 bg-white">
          <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Requests</h2>
          <ul>
            {(data?.payments ?? []).map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  onClick={() => openPayment(p.id)}
                  className={`flex w-full items-center justify-between gap-2 px-4 py-3 text-left hover:bg-paper ${selected === p.id ? "bg-paper" : ""}`}
                >
                  <span className="font-mono text-sm">{p.merchant_order_id}</span>
                  <span className="font-mono text-sm">{(p.amount_minor / 100).toFixed(2)} {p.currency}</span>
                  <span className="text-xs uppercase text-rail">{p.status}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
        <div className="border border-ink/10 bg-white">
          <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Station record</h2>
          {!detail ? (
            <p className="p-4 text-sm text-ledger">Select a payment to inspect audit, attempts, and webhooks.</p>
          ) : (
            <div className="space-y-4 p-4 text-sm">
              <div className="flex flex-wrap gap-2">
                <button type="button" className="border border-ink px-3 py-1" onClick={() => reconcile(String(detail.pay.id))}>
                  Reconcile
                </button>
              </div>
              <p>
                <strong>Status:</strong> {String(detail.pay.status)}
              </p>
              <div>
                <strong>Attempts</strong>
                <ul className="mt-1 space-y-1">
                  {detail.attempts.map((a) => (
                    <li key={a.id} className="font-mono text-xs">
                      {a.connector_key} · {a.status}
                      {a.status === "REQUIRES_ACTION" && (
                        <span className="ml-2 space-x-1">
                          <button type="button" className="underline" onClick={() => resolveAttempt(a.id, true)}>Approve</button>
                          <button type="button" className="underline" onClick={() => resolveAttempt(a.id, false)}>Reject</button>
                        </span>
                      )}
                    </li>
                  ))}
                </ul>
              </div>
              <div>
                <strong>Audit timeline</strong>
                <pre className="mt-1 max-h-40 overflow-auto font-mono text-xs">{JSON.stringify(detail.events, null, 2)}</pre>
              </div>
              <div>
                <strong>Webhook deliveries</strong>
                <pre className="mt-1 max-h-32 overflow-auto font-mono text-xs">{JSON.stringify(detail.deliveries, null, 2)}</pre>
              </div>
            </div>
          )}
        </div>
      </section>
    </main>
  );
}
