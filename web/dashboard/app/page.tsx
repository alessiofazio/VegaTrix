"use client";

import { useCallback, useEffect, useState } from "react";
import ConfigDesk from "@/components/ConfigDesk";
import SandboxLab from "@/components/SandboxLab";
import { API, api, errorDetail } from "@/lib/api";

const STATIONS = ["PENDING", "PROCESSING", "SETTLED", "FAILED", "REQUIRES_ACTION"];

type Tab = "binari" | "laboratorio" | "config";

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
  const [tab, setTab] = useState<Tab>("binari");

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
      setError(body.detail ?? "Accesso non riuscito");
      return;
    }
    setToken(body.access_token);
  }

  const refreshOverview = useCallback(async () => {
    if (!token) return;
    const res = await api<Overview>("/v1/admin/overview", token);
    if (res.ok) setData(res.body);
  }, [token]);

  useEffect(() => {
    void refreshOverview();
    const t = setInterval(() => void refreshOverview(), 5000);
    return () => clearInterval(t);
  }, [refreshOverview]);

  async function openPayment(id: string) {
    if (!token) return;
    setSelected(id);
    const [pay, events, attempts, deliveries] = await Promise.all([
      api<Record<string, unknown>>(`/v1/payment-requests/${id}`, token),
      api<unknown[]>(`/v1/payment-requests/${id}/events`, token),
      api<Detail["attempts"]>(`/v1/payment-requests/${id}/attempts`, token),
      api<unknown[]>("/v1/admin/webhook-deliveries", token),
    ]);
    setDetail({
      pay: pay.body,
      events: events.body,
      attempts: Array.isArray(attempts.body) ? attempts.body : [],
      deliveries: deliveries.body,
    });
  }

  async function reconcile(id: string) {
    if (!token) return;
    await api(`/v1/admin/payments/${id}/reconcile`, token, { method: "POST" });
    await openPayment(id);
    await refreshOverview();
  }

  async function resolveAttempt(attemptId: string, approve: boolean) {
    if (!token) return;
    const res = await api(`/v1/admin/attempts/${attemptId}/resolve`, token, {
      method: "POST",
      body: JSON.stringify({ approve }),
    });
    if (!res.ok) setError(errorDetail(res.body));
    if (selected) await openPayment(selected);
    await refreshOverview();
  }

  if (!token) {
    return (
      <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center px-6">
        <p className="text-xs uppercase tracking-[0.2em] text-rail">OpenPay Protocol · sandbox</p>
        <h1 className="mt-3 text-4xl font-semibold">Desk operatore</h1>
        <p className="mt-2 text-ledger">
          Accedi per configurare chiavi, webhook, routing e connettori, e per lanciare prove dal laboratorio.
        </p>
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
          <button type="submit" className="w-full bg-ink px-4 py-2 text-ticket">
            Entra nel desk
          </button>
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
          <h1 className="text-3xl font-semibold">Desk di controllo</h1>
        </div>
        <button type="button" className="text-sm underline" onClick={() => setToken(null)}>
          Esci
        </button>
      </header>
      <div className="rail mt-4" />
      <nav className="mt-6 flex flex-wrap gap-2 text-sm">
        {(
          [
            ["binari", "Binari"],
            ["laboratorio", "Laboratorio"],
            ["config", "Configurazione"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={`border px-3 py-1 ${tab === id ? "border-ink bg-ink text-ticket" : "border-ink/20 bg-white"}`}
          >
            {label}
          </button>
        ))}
      </nav>

      {tab === "binari" && (
        <>
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
              <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Richieste</h2>
              <ul>
                {(data?.payments ?? []).map((p) => (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => void openPayment(p.id)}
                      className={`flex w-full items-center justify-between gap-2 px-4 py-3 text-left hover:bg-paper ${selected === p.id ? "bg-paper" : ""}`}
                    >
                      <span className="font-mono text-sm">{p.merchant_order_id}</span>
                      <span className="font-mono text-sm">
                        {(p.amount_minor / 100).toFixed(2)} {p.currency}
                      </span>
                      <span className="text-xs uppercase text-rail">{p.status}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
            <div className="border border-ink/10 bg-white">
              <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">Scheda stazione</h2>
              {!detail ? (
                <p className="p-4 text-sm text-ledger">Seleziona un pagamento per audit, attempt e webhook.</p>
              ) : (
                <div className="space-y-4 p-4 text-sm">
                  <div className="flex flex-wrap gap-2">
                    <button type="button" className="border border-ink px-3 py-1" onClick={() => void reconcile(String(detail.pay.id))}>
                      Reconcile
                    </button>
                  </div>
                  <p>
                    <strong>Stato:</strong> {String(detail.pay.status)}
                  </p>
                  <div>
                    <strong>Attempt</strong>
                    <ul className="mt-1 space-y-1">
                      {detail.attempts.map((a) => (
                        <li key={a.id} className="font-mono text-xs">
                          {a.connector_key} · {a.status}
                          {a.status === "REQUIRES_ACTION" && (
                            <span className="ml-2 space-x-1 font-sans">
                              <button type="button" className="underline" onClick={() => void resolveAttempt(a.id, true)}>
                                Approva
                              </button>
                              <button type="button" className="underline" onClick={() => void resolveAttempt(a.id, false)}>
                                Rifiuta
                              </button>
                            </span>
                          )}
                        </li>
                      ))}
                    </ul>
                  </div>
                  <div>
                    <strong>Audit</strong>
                    <pre className="mt-1 max-h-40 overflow-auto font-mono text-xs">{JSON.stringify(detail.events, null, 2)}</pre>
                  </div>
                  <div>
                    <strong>Consegne webhook</strong>
                    <pre className="mt-1 max-h-32 overflow-auto font-mono text-xs">{JSON.stringify(detail.deliveries, null, 2)}</pre>
                  </div>
                </div>
              )}
            </div>
          </section>
        </>
      )}

      {tab === "laboratorio" && (
        <div className="mt-6">
          <SandboxLab token={token} />
        </div>
      )}

      {tab === "config" && (
        <div className="mt-6">
          <ConfigDesk token={token} />
        </div>
      )}
    </main>
  );
}
