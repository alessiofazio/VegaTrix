const API = import.meta.env.VITE_API_BASE_URL ?? "http://localhost:8080";
const WALLET = import.meta.env.VITE_WALLET_BASE_URL ?? "http://localhost:3003";
const KEY = "opk_demo_merchant_sandbox_not_for_production_use_only";

type Created = {
  id: string;
  status: string;
  amount_minor: number;
  currency: string;
  payment_url: string;
  qr_payload: string;
  qr_svg: string;
  expires_at: string;
};

const root = document.querySelector<HTMLElement>("#app")!;
let current: Created | null = null;
let statusText = "Nessun ordine in cassa.";

function render() {
  root.innerHTML = `
    <section class="ticket">
      <p class="eyebrow">Caffè Aurora · POS sandbox</p>
      <h1>Cassa 12,00 EUR</h1>
      <p class="lede">Crea una Payment Request, mostra il QR al wallet demo, poi osserva lo stato in cassa. Non è un pagamento reale.</p>
      <div class="actions">
        <button id="create">Crea ordine 12,00 EUR</button>
        <button id="dup" ${current ? "" : "disabled"}>Simula callback duplicato</button>
        <button id="timeout" ${current ? "" : "disabled"}>Simula timeout</button>
      </div>
      <p class="status">${statusText}</p>
      ${current ? `<div class="qr">${current.qr_svg}</div><p class="mono">${current.id} · ${current.status}</p><p><a href="${walletHref(current)}">Apri wallet demo</a></p>` : ""}
      <h2>Webhook ricevuti</h2>
      <pre id="hooks" class="mono"></pre>
    </section>`;
  document.querySelector("#create")?.addEventListener("click", createOrder);
  document.querySelector("#dup")?.addEventListener("click", duplicate);
  document.querySelector("#timeout")?.addEventListener("click", timeoutFlow);
  void loadHooks();
}

function extractToken(qr: string) {
  try {
    const normalized = qr.replace("openpay://", "https://openpay.local/");
    return new URL(normalized).searchParams.get("token") ?? "";
  } catch {
    return qr.split("token=")[1] ?? "";
  }
}

function walletHref(created: Created) {
  const token = extractToken(created.qr_payload);
  return `${WALLET}/?payment=${encodeURIComponent(created.id)}&token=${encodeURIComponent(token)}`;
}

async function createOrder() {
  const idem = crypto.randomUUID();
  const res = await fetch(`${API}/v1/payment-requests`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${KEY}`,
      "content-type": "application/json",
      "Idempotency-Key": idem,
    },
    body: JSON.stringify({
      merchant_order_id: `ORD-${Date.now()}`,
      amount_minor: 1200,
      currency: "EUR",
      description: "Espresso + cornetto",
      allowed_methods: ["ACCOUNT_TO_ACCOUNT"],
      expires_in_seconds: 300,
      metadata: { store_id: "MILANO-001", cash_register_id: "POS-04" },
    }),
  });
  current = await res.json();
  statusText = `Creato ${current?.id} in stato ${current?.status}`;
  render();
  if (current) poll(current.id);
}

async function poll(id: string) {
  const timer = window.setInterval(async () => {
    const res = await fetch(`${API}/v1/payment-requests/${id}`, {
      headers: { Authorization: `Bearer ${KEY}` },
    });
    const body = await res.json();
    if (current && current.id === id) {
      current.status = body.status;
      statusText = `Stato cassa: ${body.status}`;
      render();
      if (["SETTLED", "FAILED", "CANCELLED", "EXPIRED"].includes(body.status)) {
        window.clearInterval(timer);
      }
    }
  }, 1500);
}

async function duplicate() {
  if (!current) return;
  const token = extractToken(current.qr_payload);
  const res = await fetch(`${API}/v1/public/payments/${current.id}/simulate-duplicate`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, decision: "approve" }),
  });
  const body = await res.json();
  statusText = res.ok
    ? `Duplicate callback ignored: ${body.detail ?? body.status}`
    : `Duplicate callback error: ${JSON.stringify(body)}`;
  render();
}

async function timeoutFlow() {
  const idem = crypto.randomUUID();
  const res = await fetch(`${API}/v1/payment-requests`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${KEY}`,
      "content-type": "application/json",
      "Idempotency-Key": idem,
    },
    body: JSON.stringify({
      merchant_order_id: `ORD-TO-${Date.now()}`,
      amount_minor: 1200,
      currency: "EUR",
      description: "Timeout drill",
      allowed_methods: ["ACCOUNT_TO_ACCOUNT"],
      expires_in_seconds: 300,
      scenario: "timeout",
    }),
  });
  current = await res.json();
  statusText = "Ordine timeout creato. Approva dal wallet: il connector mock restituisce TIMEOUT e lo stato resta PROCESSING.";
  render();
}

async function loadHooks() {
  try {
    const res = await fetch("/api/webhooks");
    const body = await res.json();
    const el = document.querySelector("#hooks");
    if (el) el.textContent = JSON.stringify(body.events, null, 2);
  } catch {
    /* ignore */
  }
}

render();
