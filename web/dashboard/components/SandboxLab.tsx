"use client";

import { useCallback, useEffect, useState } from "react";
import { api, errorDetail } from "@/lib/api";

type LabStatus = {
  available: boolean;
  reason?: string | null;
  message?: string;
};

type Created = {
  id: string;
  status: string;
  amount_minor: number;
  currency: string;
  merchant_order_id?: string;
  payment_url: string;
  qr_payload: string;
  qr_svg: string;
  qr_token: string;
};

type Attempt = {
  id: string;
  status: string;
  connector_key: string;
  provider_reference?: string;
};

type Props = {
  token: string;
};

export default function SandboxLab({ token }: Props) {
  const [lab, setLab] = useState<LabStatus | null>(null);
  const [amount, setAmount] = useState("12.00");
  const [currency, setCurrency] = useState("EUR");
  const [current, setCurrent] = useState<Created | null>(null);
  const [attempts, setAttempts] = useState<Attempt[]>([]);
  const [statusText, setStatusText] = useState("Nessuna prova in corso.");
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const refreshLab = useCallback(async () => {
    const res = await api<LabStatus>("/v1/admin/sandbox", token);
    setLab(res.body);
  }, [token]);

  useEffect(() => {
    void refreshLab();
  }, [refreshLab]);

  const refreshPayment = useCallback(
    async (id: string) => {
      const [pay, atts] = await Promise.all([
        api<Created>(`/v1/payment-requests/${id}`, token),
        api<Attempt[]>(`/v1/payment-requests/${id}/attempts`, token),
      ]);
      if (pay.ok) {
        setCurrent((prev) => (prev && prev.id === id ? { ...prev, ...pay.body, id } : prev));
        setStatusText(`Stato: ${pay.body.status}`);
      }
      if (atts.ok && Array.isArray(atts.body)) {
        setAttempts(atts.body);
      }
    },
    [token],
  );

  useEffect(() => {
    if (!current?.id) return;
    const terminal = ["SETTLED", "FAILED", "CANCELLED", "EXPIRED"];
    if (terminal.includes(current.status)) return;
    const t = window.setInterval(() => void refreshPayment(current.id), 1500);
    return () => window.clearInterval(t);
  }, [current?.id, current?.status, refreshPayment]);

  async function createPayment(scenario?: string) {
    setError(null);
    const euros = Number.parseFloat(amount.replace(",", "."));
    const amountMinor = Number.isFinite(euros) ? Math.round(euros * 100) : 1200;
    const res = await api<Created>("/v1/admin/sandbox/payments", token, {
      method: "POST",
      body: JSON.stringify({
        amount_minor: amountMinor,
        currency: currency || "EUR",
        scenario: scenario ?? null,
        description: scenario === "timeout" ? "Prova timeout sandbox" : "Prova laboratorio dashboard",
        allowed_methods: ["ACCOUNT_TO_ACCOUNT"],
      }),
    });
    if (!res.ok) {
      setError(errorDetail(res.body, "Creazione pagamento non riuscita"));
      if (res.status === 403) await refreshLab();
      return;
    }
    setCurrent(res.body);
    setAttempts([]);
    setStatusText(`Creato ${res.body.id} · ${res.body.status}`);
  }

  async function authorize(decision: "approve" | "reject", scenario?: string) {
    if (!current) return;
    setError(null);
    const res = await api<Record<string, unknown>>(
      `/v1/admin/sandbox/payments/${current.id}/authorize`,
      token,
      {
        method: "POST",
        body: JSON.stringify({ decision, scenario: scenario ?? null }),
      },
    );
    if (!res.ok) {
      setError(errorDetail(res.body, "Autorizzazione non riuscita"));
      return;
    }
    await refreshPayment(current.id);
  }

  async function duplicate() {
    if (!current) return;
    setError(null);
    const res = await api<Record<string, unknown>>(
      `/v1/admin/sandbox/payments/${current.id}/duplicate`,
      token,
      { method: "POST" },
    );
    setStatusText(
      res.ok
        ? `Callback duplicato: ${String(res.body.detail ?? res.body.status ?? "ok")}`
        : errorDetail(res.body),
    );
    if (!res.ok) setError(errorDetail(res.body));
    await refreshPayment(current.id);
  }

  async function reconcile() {
    if (!current) return;
    setError(null);
    const res = await api<Record<string, unknown>>(
      `/v1/admin/payments/${current.id}/reconcile`,
      token,
      { method: "POST" },
    );
    if (!res.ok) setError(errorDetail(res.body, "Reconcile non riuscito"));
    await refreshPayment(current.id);
  }

  async function resolveAttempt(id: string, approve: boolean) {
    setError(null);
    const res = await api<Record<string, unknown>>(`/v1/admin/attempts/${id}/resolve`, token, {
      method: "POST",
      body: JSON.stringify({ approve }),
    });
    if (!res.ok) setError(errorDetail(res.body, "Resolve manuale non riuscito"));
    if (current) await refreshPayment(current.id);
  }

  async function copyLink() {
    if (!current?.payment_url) return;
    await navigator.clipboard.writeText(current.payment_url);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  const blocked = lab && !lab.available;

  return (
    <section className="border border-ink/10 bg-white">
      <h2 className="border-b border-ink/10 px-4 py-3 text-sm uppercase tracking-widest">
        Laboratorio sandbox
      </h2>
      <div className="space-y-4 p-4 text-sm">
        <p className="text-ledger">
          Crea un pagamento del merchant demo, mostra QR e link wallet, poi prova Approva / Rifiuta /
          timeout / callback duplicato. Non è un PSP live.
        </p>
        {lab && (
          <p className={lab.available ? "text-rail" : "text-signal"}>{lab.message}</p>
        )}
        <div className="flex flex-wrap items-end gap-3">
          <label className="block">
            Importo
            <input
              className="mt-1 block w-28 border border-ink/20 px-2 py-1 font-mono"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
            />
          </label>
          <label className="block">
            Valuta
            <input
              className="mt-1 block w-20 border border-ink/20 px-2 py-1 font-mono uppercase"
              maxLength={3}
              value={currency}
              onChange={(e) => setCurrency(e.target.value.toUpperCase())}
            />
          </label>
          <button
            type="button"
            disabled={!!blocked}
            className="border border-ink bg-ink px-3 py-1 text-ticket disabled:opacity-40"
            onClick={() => void createPayment()}
          >
            Crea prova
          </button>
          <button
            type="button"
            disabled={!!blocked}
            className="border border-ink px-3 py-1 disabled:opacity-40"
            onClick={() => void createPayment("timeout")}
          >
            Simula timeout
          </button>
        </div>
        <div className="flex flex-wrap gap-2">
          <button type="button" disabled={!current || !!blocked} className="border border-ink px-3 py-1 disabled:opacity-40" onClick={() => void authorize("approve")}>
            Approva
          </button>
          <button type="button" disabled={!current || !!blocked} className="border border-ink px-3 py-1 disabled:opacity-40" onClick={() => void authorize("reject")}>
            Rifiuta
          </button>
          <button type="button" disabled={!current || !!blocked} className="border border-ink px-3 py-1 disabled:opacity-40" onClick={() => void duplicate()}>
            Simula callback duplicato
          </button>
          <button type="button" disabled={!current} className="border border-ink px-3 py-1 disabled:opacity-40" onClick={() => void reconcile()}>
            Reconcile
          </button>
        </div>
        {error && <p className="text-signal">{error}</p>}
        <p className="font-mono text-xs">{statusText}</p>
        {current && (
          <div className="grid gap-4 md:grid-cols-2">
            <div>
              <div className="qr max-w-xs" dangerouslySetInnerHTML={{ __html: current.qr_svg }} />
              <p className="mt-2 font-mono text-xs">
                {current.id} · {(current.amount_minor / 100).toFixed(2)} {current.currency}
              </p>
              <div className="mt-2 flex flex-wrap gap-2">
                <a className="underline" href={current.payment_url} target="_blank" rel="noreferrer">
                  Apri wallet
                </a>
                <button type="button" className="underline" onClick={() => void copyLink()}>
                  {copied ? "Copiato" : "Copia link"}
                </button>
              </div>
            </div>
            <div>
              <strong>Attempt</strong>
              <ul className="mt-1 space-y-1 font-mono text-xs">
                {attempts.length === 0 && <li className="text-ledger">Nessun attempt ancora.</li>}
                {attempts.map((a) => (
                  <li key={a.id}>
                    {a.connector_key} · {a.status}
                    {a.status === "REQUIRES_ACTION" && (
                      <span className="ml-2 space-x-2 font-sans">
                        <button type="button" className="underline" onClick={() => void resolveAttempt(a.id, true)}>
                          Approva manuale
                        </button>
                        <button type="button" className="underline" onClick={() => void resolveAttempt(a.id, false)}>
                          Rifiuta manuale
                        </button>
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
