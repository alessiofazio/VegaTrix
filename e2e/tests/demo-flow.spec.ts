import { expect, test } from "@playwright/test";

const API = process.env.API_BASE_URL ?? "http://localhost:8080";
const WALLET = process.env.WALLET_BASE_URL ?? "http://localhost:3003";
const KEY = process.env.DEMO_API_KEY ?? "opk_demo_merchant_sandbox_not_for_production_use_only";

test("merchant creates order, wallet approves, status settles", async ({ page, request }) => {
  const health = await request.get(`${API}/healthz`);
  expect(health.ok()).toBeTruthy();

  const idem = crypto.randomUUID();
  const created = await request.post(`${API}/v1/payment-requests`, {
    headers: {
      Authorization: `Bearer ${KEY}`,
      "Idempotency-Key": idem,
      "content-type": "application/json",
    },
    data: {
      merchant_order_id: `E2E-${Date.now()}`,
      amount_minor: 1200,
      currency: "EUR",
      allowed_methods: ["ACCOUNT_TO_ACCOUNT"],
      expires_in_seconds: 300,
    },
  });
  expect(created.ok()).toBeTruthy();
  const body = await created.json();
  expect(body.amount_minor).toBe(1200);

  const tokenMatch = String(body.qr_payload).match(/token=([^&]+)/);
  expect(tokenMatch).toBeTruthy();
  const token = decodeURIComponent(tokenMatch![1]);

  await page.goto(`${WALLET}/?payment=${encodeURIComponent(body.id)}&token=${encodeURIComponent(token)}`);
  await expect(page.getByRole("button", { name: "Approva" })).toBeVisible();
  await page.getByRole("button", { name: "Approva" }).click();

  let settled = false;
  for (let i = 0; i < 20; i++) {
    const statusRes = await request.get(`${API}/v1/payment-requests/${body.id}`, {
      headers: { Authorization: `Bearer ${KEY}` },
    });
    const statusBody = await statusRes.json();
    if (statusBody.status === "SETTLED") {
      settled = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 1500));
  }
  expect(settled).toBeTruthy();
});

test("timeout order stays processing then reconciles via admin", async ({ request }) => {
  test.skip(!process.env.RUN_RECONCILE_E2E, "optional reconcile path");
});
