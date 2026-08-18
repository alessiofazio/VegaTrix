/**
 * Thin HTTP wrapper for the OpenPay merchant API (v1 sandbox).
 * No business logic — see docs/api/API-GUIDE.md.
 *
 * Auth: `Authorization: Bearer opk_…` (API key) or a JWT access token.
 * Create requires header `Idempotency-Key`. JSON is snake_case; amounts are integer minor units.
 */
import { createHmac } from "node:crypto";

export type CreatePaymentInput = {
  merchant_order_id: string;
  amount_minor: number;
  currency: string;
  description?: string;
  allowed_methods?: string[];
  expires_in_seconds?: number;
  return_url?: string;
  metadata?: Record<string, unknown>;
  scenario?: string;
};

export type Payment = {
  id: string;
  status: string;
  amount_minor: number;
  currency: string;
  payment_url?: string;
  qr_payload?: string;
  qr_svg?: string;
  /** Unix seconds (`time` crate serde). Create also returns `created_at`, `replayed`. */
  expires_at?: number | string;
};

export class OpenPay {
  /**
   * @param baseUrl e.g. `http://localhost:8080` (sandbox)
   * @param apiKey merchant secret starting with `opk_` — demo:
   *   `opk_demo_merchant_sandbox_not_for_production_use_only` (not for production)
   */
  constructor(
    private readonly baseUrl: string,
    private readonly apiKey: string,
  ) {}

  private headers(idempotencyKey?: string): Record<string, string> {
    const h: Record<string, string> = {
      Authorization: `Bearer ${this.apiKey}`,
      "content-type": "application/json",
    };
    if (idempotencyKey) h["Idempotency-Key"] = idempotencyKey;
    return h;
  }

  async createPayment(idempotencyKey: string, input: CreatePaymentInput): Promise<Payment> {
    const res = await fetch(`${this.baseUrl}/v1/payment-requests`, {
      method: "POST",
      headers: this.headers(idempotencyKey),
      body: JSON.stringify(input),
    });
    if (!res.ok) throw new Error(`OpenPay error ${res.status}: ${await res.text()}`);
    return (await res.json()) as Payment;
  }

  async getPayment(id: string): Promise<Payment> {
    const res = await fetch(`${this.baseUrl}/v1/payment-requests/${id}`, {
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(`OpenPay error ${res.status}: ${await res.text()}`);
    return (await res.json()) as Payment;
  }

  async cancelPayment(id: string): Promise<Payment> {
    const res = await fetch(`${this.baseUrl}/v1/payment-requests/${id}/cancel`, {
      method: "POST",
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(`OpenPay error ${res.status}: ${await res.text()}`);
    return (await res.json()) as Payment;
  }

  async refundPayment(id: string): Promise<Payment> {
    const res = await fetch(`${this.baseUrl}/v1/payment-requests/${id}/refunds`, {
      method: "POST",
      headers: this.headers(),
    });
    if (!res.ok) throw new Error(`OpenPay error ${res.status}: ${await res.text()}`);
    return (await res.json()) as Payment;
  }

  /**
   * Verify `OpenPay-Signature: t=<unix>,v1=<hex>` over `{t}.{rawBody}` (HMAC-SHA256).
   * Use the raw request body; default tolerance 300s (`WEBHOOK_TOLERANCE_SECS`).
   */
  verifyWebhookSignature(secret: string, header: string, rawBody: string, toleranceSecs = 300): boolean {
    const parts = Object.fromEntries(header.split(",").map((p) => p.trim().split("=") as [string, string]));
    const t = parts.t;
    const v1 = parts.v1;
    if (!t || !v1) return false;
    const now = Math.floor(Date.now() / 1000);
    if (Math.abs(now - Number(t)) > toleranceSecs) return false;
    const expected = createHmac("sha256", secret).update(`${t}.`).update(rawBody).digest("hex");
    return expected === v1;
  }
}
