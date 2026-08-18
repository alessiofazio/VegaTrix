const API = import.meta.env.VITE_API_BASE_URL ?? "http://localhost:8080";
const params = new URLSearchParams(window.location.search);
let payment = params.get("payment") ?? "";
let token = params.get("token") ?? "";

if (!payment && window.location.hash.startsWith("#openpay://")) {
  try {
    const uri = window.location.hash.slice(1);
    const u = new URL(uri.replace("openpay://", "https://openpay.local/"));
    payment = u.pathname.split("/").pop() ?? "";
    token = u.searchParams.get("token") ?? "";
  } catch {
    /* ignore */
  }
}

const root = document.querySelector("#app")!;

async function load() {
  if (!payment || !token) {
    root.innerHTML = `<section class="card"><h1>Wallet sandbox</h1><p>Apri un payment link dal merchant demo. Questo wallet non muove denaro reale.</p></section>`;
    return;
  }
  const res = await fetch(`${API}/v1/public/payments/${payment}?token=${encodeURIComponent(token)}`);
  const body = await res.json();
  if (!res.ok) {
    root.innerHTML = `<section class="card"><h1>Token non valido</h1><p>${body.detail ?? res.status}</p></section>`;
    return;
  }
  root.innerHTML = `
    <section class="card">
      <p class="eyebrow">Wallet demo · sandbox</p>
      <h1>${body.merchant_display_name}</h1>
      <p class="amount">${(body.amount_minor / 100).toFixed(2)} <span>${body.currency}</span></p>
      <p>Scade ${body.expires_at}</p>
      <p>${body.description ?? ""}</p>
      <p class="status">Stato corrente: ${body.status}</p>
      <div class="row">
        <button id="ok">Approva</button>
        <button id="no" class="ghost">Rifiuta</button>
      </div>
      <p id="out"></p>
    </section>`;
  document.querySelector("#ok")?.addEventListener("click", () => decide("approve"));
  document.querySelector("#no")?.addEventListener("click", () => decide("reject"));
}

async function decide(decision: string) {
  const out = document.querySelector("#out");
  const res = await fetch(`${API}/v1/public/payments/${payment}/authorize`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, decision }),
  });
  const body = await res.json();
  if (out) out.textContent = JSON.stringify(body, null, 2);
}

void load();
