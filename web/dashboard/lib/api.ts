const API = process.env.NEXT_PUBLIC_API_BASE_URL ?? "http://localhost:8080";

export { API };

export async function api<T>(
  path: string,
  token: string,
  init?: RequestInit,
): Promise<{ ok: boolean; status: number; body: T }> {
  const headers = new Headers(init?.headers);
  headers.set("Authorization", `Bearer ${token}`);
  if (init?.body && !headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }
  const res = await fetch(`${API}${path}`, { ...init, headers });
  const text = await res.text();
  let body: T;
  try {
    body = text ? (JSON.parse(text) as T) : ({} as T);
  } catch {
    body = { detail: text } as T;
  }
  return { ok: res.ok, status: res.status, body };
}

export function errorDetail(body: unknown, fallback = "Operazione non riuscita"): string {
  if (body && typeof body === "object") {
    const rec = body as Record<string, unknown>;
    if (typeof rec.detail === "string") return rec.detail;
    if (typeof rec.title === "string") return rec.title;
    if (typeof rec.message === "string") return rec.message;
  }
  return fallback;
}
