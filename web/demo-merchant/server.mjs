import express from "express";
import { readFileSync, existsSync } from "node:fs";
import { createHmac } from "node:crypto";
import { createServer as createVite } from "vite";

const PORT = Number(process.env.PORT ?? 3002);
const secret = process.env.OPENPAY_WEBHOOK_SECRET ?? "replace_me_webhook_signing";
const events = [];

function verifySignature(raw, header) {
  const parts = Object.fromEntries(
    (header ?? "").split(",").map((p) => p.trim().split("=")),
  );
  const t = parts.t;
  const v1 = parts.v1;
  if (!t || !v1) return false;
  const expected = createHmac("sha256", secret).update(`${t}.`).update(raw).digest("hex");
  return expected === v1;
}

const app = express();
app.use(express.raw({ type: "application/json" }));

app.post("/webhooks/openpay", (req, res) => {
  const raw = Buffer.isBuffer(req.body) ? req.body : Buffer.from(JSON.stringify(req.body ?? {}));
  const ok = verifySignature(raw, req.header("OpenPay-Signature"));
  const payload = JSON.parse(raw.toString("utf8") || "{}");
  events.unshift({ received_at: new Date().toISOString(), verified: ok, payload });
  events.splice(50);
  res.status(ok ? 200 : 400).json({ ok });
});

app.get("/api/webhooks", (_req, res) => {
  res.json({ events });
});

app.use(express.json());

const vite = await createVite({ server: { middlewareMode: true } });
app.use(vite.middlewares);

app.use("*", async (req, res, next) => {
  if (req.originalUrl.startsWith("/webhooks") || req.originalUrl.startsWith("/api")) return next();
  try {
    const template = existsSync("index.html") ? readFileSync("index.html", "utf8") : "<div id='app'></div>";
    const html = await vite.transformIndexHtml(req.originalUrl, template);
    res.status(200).set({ "content-type": "text/html" }).end(html);
  } catch (e) {
    next(e);
  }
});

app.listen(PORT, () => console.log(`demo-merchant on ${PORT}`));
